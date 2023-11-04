//! This very specific to 64-bit x86 architecture, if this is to be ported to other architectures
//! this will need to be changed

use crate::{
    cpu,
    memory_management::memory_layout::{is_aligned, PAGE_2M},
    sync::spin::mutex::Mutex,
};

use super::{
    memory_layout::{
        align_down, align_up, kernel_rodata_end, physical2virtual, virtual2physical,
        EXTENDED_OFFSET, KERNEL_BASE, KERNEL_LINK, PAGE_4K,
    },
    physical_page_allocator,
};

// TODO: replace by some sort of bitfield
#[allow(dead_code)]
pub mod flags {
    pub(super) const PTE_PRESENT: u64 = 1 << 0;
    pub const PTE_WRITABLE: u64 = 1 << 1;
    pub const PTE_USER: u64 = 1 << 2;
    pub const PTE_WRITETHROUGH: u64 = 1 << 3;
    pub const PTE_NOT_CACHEABLE: u64 = 1 << 4;
    pub(super) const PTE_ACCESSED: u64 = 1 << 5;
    pub(super) const PTE_DIRTY: u64 = 1 << 6;
    pub(super) const PTE_HUGE_PAGE: u64 = 1 << 7;
    pub(super) const PTE_GLOBAL: u64 = 1 << 8;
    pub(super) const PTE_NO_EXECUTE: u64 = 1 << 63;
}

// have a specific alignment so we can fit them in a page
#[repr(C, align(32))]
#[derive(Debug, Copy, Clone)]
pub struct VirtualMemoryMapEntry {
    pub virtual_address: u64,
    pub physical_address: u64,
    pub size: usize,
    pub flags: u64,
}

// This is a general structure for all levels
#[repr(C, align(4096))]
struct PageDirectoryTable {
    entries: [u64; 512],
}

static mut VIRTUAL_MEMORY_MANAGER: Mutex<VirtualMemoryManager> =
    Mutex::new(VirtualMemoryManager::empty());

pub fn init_kernel_vm() {
    unsafe {
        VIRTUAL_MEMORY_MANAGER.lock().init_kernel_vm();
    }
}

#[allow(dead_code)]
pub fn map(entry: &VirtualMemoryMapEntry) {
    unsafe {
        VIRTUAL_MEMORY_MANAGER.lock().map(entry);
    }
}

struct VirtualMemoryManager {
    page_map_l4: *mut PageDirectoryTable,
}

impl VirtualMemoryManager {
    pub const fn empty() -> Self {
        Self {
            // use the same address we used in the assembly code
            // we will change this anyway in `init`, but at least lets have a valid address
            page_map_l4: physical2virtual(0x1000) as *mut _,
        }
    }

    // This replicate what is done in the assembly code
    // but it will be stored
    fn init_kernel_vm(&mut self) {
        let data_start = align_up(kernel_rodata_end() as *mut u8, PAGE_4K) as usize;
        let read_only_data_size = kernel_rodata_end() - KERNEL_LINK;
        // TODO: for now we are mapping the first 128MB of memory
        //       this is not good enough, as it might be less than the actual memory
        //       also, we are not utilizing the whole memory like this
        //       find a better way to map memory dynamically using the data from the multiboot info
        let whole_memory_to_map = 128 * 1024 * 1024; // 128 MB
        let whole_memory_after_rodata = whole_memory_to_map - read_only_data_size - EXTENDED_OFFSET;
        let kernel_vm = [
            // Low memory (has some BIOS stuff): mapped to kernel space
            VirtualMemoryMapEntry {
                virtual_address: KERNEL_BASE as u64,
                physical_address: 0,
                size: EXTENDED_OFFSET,
                flags: flags::PTE_WRITABLE,
            },
            // Extended memory: kernel .text and .rodata sections
            VirtualMemoryMapEntry {
                virtual_address: KERNEL_LINK as u64,
                physical_address: virtual2physical(KERNEL_LINK) as u64,
                size: read_only_data_size,
                flags: 0, // read-only
            },
            // Extended memory: kernel .data and .bss sections and the rest of the data for the `whole` memory
            // we decided to use in the kernel
            VirtualMemoryMapEntry {
                virtual_address: data_start as u64,
                physical_address: virtual2physical(data_start) as u64,
                size: whole_memory_after_rodata,
                flags: flags::PTE_WRITABLE,
            },
        ];

        // create a new fresh page map
        // SAFETY: we are calling the virtual memory manager after initializing the physical page allocator
        self.page_map_l4 = unsafe { physical_page_allocator::alloc_zeroed() } as *mut _;

        for entry in kernel_vm.iter() {
            self.map(entry);
        }

        self.switch_vm(self.page_map_l4);
    }

    fn switch_vm(&mut self, base: *mut PageDirectoryTable) {
        unsafe { cpu::set_cr3(virtual2physical(base as _) as _) }
    }

    fn map(&mut self, entry: &VirtualMemoryMapEntry) {
        let VirtualMemoryMapEntry {
            mut virtual_address,
            mut physical_address,
            mut size,
            flags,
        } = entry;

        assert!(!self.page_map_l4.is_null());
        assert!(is_aligned(self.page_map_l4 as _, PAGE_4K));
        assert!(size > 0);

        virtual_address = align_down(virtual_address as _, PAGE_4K) as _;
        physical_address = align_down(physical_address as _, PAGE_4K) as _;
        size = align_up(size as _, PAGE_4K) as _;

        eprintln!(
            "{:08X?}",
            VirtualMemoryMapEntry {
                virtual_address: virtual_address as _,
                physical_address: physical_address as _,
                size: size as _,
                flags: *flags,
            }
        );

        while size > 0 {
            let page_map_l4_index = ((virtual_address >> 39) & 0x1FF) as usize;
            let page_directory_pointer_index = ((virtual_address >> 30) & 0x1FF) as usize;
            let page_directory_index = ((virtual_address >> 21) & 0x1FF) as usize;
            let page_table_index = ((virtual_address >> 12) & 0x1FF) as usize;

            // Level 4
            let page_map_l4 = unsafe { &mut *self.page_map_l4 };
            let page_map_l4_entry = &mut page_map_l4.entries[page_map_l4_index];

            if *page_map_l4_entry & flags::PTE_PRESENT == 0 {
                let page_directory_pointer_table =
                    unsafe { physical_page_allocator::alloc_zeroed() as *mut PageDirectoryTable };
                *page_map_l4_entry = (virtual2physical(page_directory_pointer_table as _) as u64
                    & 0x00000000FFFFF000)
                    | flags::PTE_PRESENT;
            }
            // add new flags if any
            *page_map_l4_entry |= flags;
            eprintln!(
                "L4[{}]: {:p} = {:x}",
                page_map_l4_index, page_map_l4_entry, *page_map_l4_entry
            );

            // Level 3
            let page_directory_pointer_table = unsafe {
                &mut *((physical2virtual((*page_map_l4_entry & 0x00000000FFFFF000) as usize))
                    as *mut PageDirectoryTable)
            };

            let page_directory_pointer_entry =
                &mut page_directory_pointer_table.entries[page_directory_pointer_index];

            if *page_directory_pointer_entry & flags::PTE_PRESENT == 0 {
                let page_directory_table =
                    unsafe { physical_page_allocator::alloc_zeroed() as *mut PageDirectoryTable };

                *page_directory_pointer_entry =
                    (virtual2physical(page_directory_table as _) as u64 & 0x00000000FFFFF000)
                        | flags::PTE_PRESENT;
            }

            // add new flags
            *page_directory_pointer_entry |= flags;
            eprintln!(
                "L3[{}]: {:p} = {:x}",
                page_directory_pointer_index,
                page_directory_pointer_entry,
                *page_directory_pointer_entry
            );

            // Level 2
            let page_directory_table = unsafe {
                &mut *(physical2virtual(
                    (*page_directory_pointer_entry & 0x00000000FFFFF000) as usize,
                ) as *mut PageDirectoryTable)
            };
            let page_directory_entry = &mut page_directory_table.entries[page_directory_index];

            // here we have an intersection, if we can map a 2MB page, we will, otherwise we will map a 4K page
            let can_map_2mb_page = is_aligned(physical_address as _, PAGE_2M)
                && is_aligned(virtual_address as _, PAGE_2M)
                && size >= PAGE_2M;

            if can_map_2mb_page {
                // we already have an entry here
                if *page_directory_entry & flags::PTE_PRESENT != 0 {
                    // did we have a mapping here that lead to 4k pages?
                    // if so, we should free the physical page allocation for them
                    if *page_directory_entry & flags::PTE_HUGE_PAGE == 0 {
                        let page_table_ptr =
                            (*page_directory_entry & 0x00000000FFFFF000) as *mut PageDirectoryTable;

                        unsafe { physical_page_allocator::free(page_table_ptr as _) };
                    }
                }

                // Level 1
                *page_directory_entry = (physical_address & 0x00000000FFFFF000)
                    | flags
                    | flags::PTE_PRESENT
                    | flags::PTE_HUGE_PAGE;

                eprintln!(
                    "L2[{}] huge: {:p} = {:x}",
                    page_directory_index, page_directory_entry, *page_directory_entry
                );

                size -= PAGE_2M;
                virtual_address += PAGE_2M as u64;
                physical_address += PAGE_2M as u64;
            } else {
                // continue mapping 4K pages
                if *page_directory_entry & flags::PTE_PRESENT == 0 {
                    let page_table = unsafe {
                        physical_page_allocator::alloc_zeroed() as *mut PageDirectoryTable
                    };
                    *page_directory_entry = (virtual2physical(page_table as _) as u64
                        & 0x00000000FFFFF000)
                        | flags::PTE_PRESENT;
                }
                // add new flags
                *page_directory_entry |= flags;
                eprintln!(
                    "L2[{}]: {:p} = {:x}",
                    page_directory_index, page_directory_entry, *page_directory_entry
                );

                // Level 1
                let page_table = unsafe {
                    &mut *(physical2virtual((*page_directory_entry & 0x00000000FFFFF000) as usize)
                        as *mut PageDirectoryTable)
                };
                let page_table_entry = &mut page_table.entries[page_table_index];
                *page_table_entry =
                    (physical_address & 0x00000000FFFFF000) | flags | flags::PTE_PRESENT;
                eprintln!(
                    "L1[{}]: {:p} = {:x}",
                    page_table_index, page_table_entry, *page_table_entry
                );

                size -= PAGE_4K;
                virtual_address += PAGE_4K as u64;
                physical_address += PAGE_4K as u64;
            }

            eprintln!();
        }
    }
}
