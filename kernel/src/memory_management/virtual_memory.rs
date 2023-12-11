//! This very specific to 64-bit x86 architecture, if this is to be ported to other architectures
//! this will need to be changed

use core::slice::IterMut;

use crate::{
    cpu,
    memory_management::{
        memory_layout::{
            align_down, align_up, is_aligned, kernel_elf_rodata_end, physical2virtual,
            virtual2physical, MemSize, DEVICE_BASE_PHYSICAL, DEVICE_BASE_VIRTUAL,
            DEVICE_PHYSICAL_END, EXTENDED_BIOS_BASE_PHYSICAL, EXTENDED_BIOS_BASE_VIRTUAL,
            EXTENDED_BIOS_SIZE, EXTENDED_OFFSET, KERNEL_BASE, KERNEL_LINK, KERNEL_MAPPED_SIZE,
            PAGE_2M, PAGE_4K,
        },
        physical_page_allocator,
    },
    sync::spin::mutex::Mutex,
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

const ADDR_MASK: u64 = 0x0000_0000_FFFF_F000;

// only use the last index for the kernel
// all the other indexes are free to use by the user
const KERNEL_L4_INDEX: usize = 0x1FF;

#[inline(always)]
const fn get_l4(addr: u64) -> u64 {
    (addr >> 39) & 0x1FF
}

#[inline(always)]
const fn get_l3(addr: u64) -> u64 {
    (addr >> 30) & 0x1FF
}

#[inline(always)]
const fn get_l2(addr: u64) -> u64 {
    (addr >> 21) & 0x1FF
}

#[inline(always)]
const fn get_l1(addr: u64) -> u64 {
    (addr >> 12) & 0x1FF
}

// have a specific alignment so we can fit them in a page
#[repr(C, align(32))]
#[derive(Debug, Copy, Clone)]
pub struct VirtualMemoryMapEntry {
    pub virtual_address: u64,
    pub physical_address: Option<u64>,
    pub size: u64,
    pub flags: u64,
}

// This is a general structure for all levels
#[repr(C, align(4096))]
struct PageDirectoryTable {
    entries: [u64; 512],
}

#[repr(transparent)]
struct PageDirectoryTablePtr(pub u64);

impl PageDirectoryTablePtr {
    fn from_entry(entry: u64) -> Self {
        Self(physical2virtual((entry & ADDR_MASK) as _) as _)
    }

    /// An ugly hack used in `do_for_every_user_entry` to get a mutable reference to the page directory table
    fn enteries_from_mut_entry(entry: &mut u64) -> &mut PageDirectoryTable {
        let table = physical2virtual((*entry & ADDR_MASK) as _) as *mut PageDirectoryTable;
        unsafe { &mut *table }
    }

    fn to_physical(&self) -> u64 {
        virtual2physical(self.0 as _) as _
    }

    fn alloc_new() -> Self {
        // SAFETY: it will panic if it couldn't allocate, so if it returns, it is safe
        Self(unsafe { physical_page_allocator::alloc_zeroed() } as _)
    }

    fn as_ptr(&self) -> *mut PageDirectoryTable {
        self.0 as *mut PageDirectoryTable
    }

    fn as_mut(&mut self) -> &mut PageDirectoryTable {
        unsafe { &mut *self.as_ptr() }
    }

    fn as_ref(&self) -> &PageDirectoryTable {
        unsafe { &*self.as_ptr() }
    }

    unsafe fn free(self) {
        unsafe { physical_page_allocator::free(self.0 as _) };
    }
}

static KERNEL_VIRTUAL_MEMORY_MANAGER: Mutex<VirtualMemoryManager> =
    Mutex::new(VirtualMemoryManager::boot_vm());

pub fn init_kernel_vm() {
    let new_kernel_manager = VirtualMemoryManager::new_kernel_vm();
    let mut manager = KERNEL_VIRTUAL_MEMORY_MANAGER.lock();
    *manager = new_kernel_manager;
    manager.switch_to_this();

    // map the BIOS memory
    map_bios_memory(&mut manager);
    map_device_memory(&mut manager);
}

fn map_bios_memory(manager: &mut VirtualMemoryManager) {
    assert!(unsafe { EXTENDED_BIOS_BASE_PHYSICAL } > 0 && unsafe { EXTENDED_BIOS_SIZE } > 0);
    // the space immediately after the ram is reserved for the BIOS
    let map_entry = VirtualMemoryMapEntry {
        virtual_address: EXTENDED_BIOS_BASE_VIRTUAL as u64,
        physical_address: Some(unsafe { EXTENDED_BIOS_BASE_PHYSICAL } as u64),
        size: unsafe { EXTENDED_BIOS_SIZE } as u64,
        flags: 0,
    };

    manager.map(&map_entry);
}

fn map_device_memory(manager: &mut VirtualMemoryManager) {
    let map_entry = VirtualMemoryMapEntry {
        virtual_address: DEVICE_BASE_VIRTUAL as u64,
        physical_address: Some(DEVICE_BASE_PHYSICAL as u64),
        size: DEVICE_PHYSICAL_END as u64 - DEVICE_BASE_PHYSICAL as u64,
        flags: flags::PTE_WRITABLE,
    };

    manager.map(&map_entry);
}

#[allow(dead_code)]
pub fn switch_to_kernel() {
    KERNEL_VIRTUAL_MEMORY_MANAGER.lock().switch_to_this();
}

pub fn map_kernel(entry: &VirtualMemoryMapEntry) {
    // make sure we are only mapping to kernel memory
    assert!(entry.virtual_address >= KERNEL_BASE as u64);
    KERNEL_VIRTUAL_MEMORY_MANAGER.lock().map(entry);
}

#[allow(dead_code)]
pub fn is_address_mapped_in_kernel(addr: u64) -> bool {
    KERNEL_VIRTUAL_MEMORY_MANAGER.lock().is_address_mapped(addr)
}

#[allow(dead_code)]
pub fn clone_kernel_vm_as_user() -> VirtualMemoryManager {
    let manager = KERNEL_VIRTUAL_MEMORY_MANAGER.lock();
    let mut new_vm = manager.clone_kernel_mem();
    new_vm.is_user = true;
    new_vm
}

pub fn get_current_vm() -> VirtualMemoryManager {
    VirtualMemoryManager::get_current_vm()
}

pub struct VirtualMemoryManager {
    page_map_l4: PageDirectoryTablePtr,
    is_user: bool,
}

impl VirtualMemoryManager {
    /// Return the VM for the CPU at boot time (only applied to the first CPU and this is setup in `boot.S`)
    const fn boot_vm() -> Self {
        Self {
            // use the same address we used in the assembly code
            // we will change this anyway in `new_kernel_vm`, but at least lets have a valid address
            page_map_l4: PageDirectoryTablePtr(physical2virtual(0x1000) as _),
            is_user: false,
        }
    }

    fn new() -> Self {
        Self {
            page_map_l4: PageDirectoryTablePtr::alloc_new(),
            is_user: false,
        }
    }

    // create a new virtual memory that maps the kernel only
    pub fn clone_kernel_mem(&self) -> Self {
        let mut new_vm = Self::new();

        // share the same kernel mapping
        new_vm.page_map_l4.as_mut().entries[KERNEL_L4_INDEX] =
            (self.page_map_l4.as_ref()).entries[KERNEL_L4_INDEX];

        new_vm
    }

    fn load_vm(base: &PageDirectoryTablePtr) {
        eprintln!(
            "Switching to new page map: {:p}",
            virtual2physical(base.0 as _) as *const u8
        );
        unsafe { cpu::set_cr3(base.to_physical()) }
    }

    fn get_current_vm() -> Self {
        let kernel_vm_addr = KERNEL_VIRTUAL_MEMORY_MANAGER.lock().page_map_l4.0;
        let cr3 = physical2virtual(unsafe { cpu::get_cr3() } as _) as _;
        let is_user = cr3 != kernel_vm_addr;
        Self {
            page_map_l4: PageDirectoryTablePtr(cr3),
            is_user,
        }
    }

    pub fn switch_to_this(&self) {
        Self::load_vm(&self.page_map_l4);
    }

    // This replicate what is done in the assembly code
    // but it will be stored
    fn new_kernel_vm() -> Self {
        let data_start = align_up(kernel_elf_rodata_end(), PAGE_4K);
        let kernel_vm = [
            // Low memory (has some BIOS stuff): mapped to kernel space
            VirtualMemoryMapEntry {
                virtual_address: KERNEL_BASE as u64,
                physical_address: Some(0),
                size: EXTENDED_OFFSET as u64,
                flags: flags::PTE_WRITABLE,
            },
            // Extended memory: kernel .text and .rodata sections
            VirtualMemoryMapEntry {
                virtual_address: KERNEL_LINK as u64,
                physical_address: Some(virtual2physical(KERNEL_LINK) as u64),
                size: virtual2physical(data_start) as u64 - virtual2physical(KERNEL_LINK) as u64,
                flags: 0, // read-only
            },
            // Extended memory: kernel .data and .bss sections and the rest of the data for the `whole` memory
            // we decided to use in the kernel
            VirtualMemoryMapEntry {
                virtual_address: data_start as u64,
                physical_address: Some(virtual2physical(data_start) as u64),
                size: KERNEL_MAPPED_SIZE as u64 - virtual2physical(data_start) as u64,
                flags: flags::PTE_WRITABLE,
            },
        ];

        // create a new fresh page map
        // SAFETY: we are calling the virtual memory manager after initializing the physical page allocator
        let mut s = Self::new();

        for entry in kernel_vm.iter() {
            s.map(entry);
        }

        s
    }

    pub fn map(&mut self, entry: &VirtualMemoryMapEntry) {
        let VirtualMemoryMapEntry {
            mut virtual_address,
            physical_address: mut start_physical_address,
            mut size,
            flags,
        } = entry;

        assert!(!self.page_map_l4.as_ptr().is_null());
        assert!(is_aligned(self.page_map_l4.0 as _, PAGE_4K));
        if self.is_user {
            assert!(*flags & flags::PTE_USER != 0);
            assert!(virtual_address < KERNEL_BASE as u64);
        }
        // get the end before alignment
        let end_virtual_address = (virtual_address - 1) + size;
        virtual_address = align_down(virtual_address as _, PAGE_4K) as _;
        start_physical_address =
            start_physical_address.map(|addr| align_down(addr as _, PAGE_4K) as _);
        size = align_up((end_virtual_address - virtual_address) as _, PAGE_4K) as _;

        // keep track of current address and size
        let mut physical_address = start_physical_address;

        assert!(size > 0);

        eprintln!(
            "{} {:08X?}",
            MemSize(size),
            VirtualMemoryMapEntry {
                virtual_address: virtual_address as _,
                physical_address: physical_address as _,
                size,
                flags: *flags,
            }
        );

        while size > 0 {
            let current_physical_address = physical_address.unwrap_or_else(|| {
                virtual2physical(unsafe { physical_page_allocator::alloc_zeroed() as _ }) as _
            });
            eprintln!(
                "[!] Mapping {:p} to {:p}",
                virtual_address as *const u8, current_physical_address as *const u8
            );
            let page_map_l4_index = get_l4(virtual_address) as usize;
            let page_directory_pointer_index = get_l3(virtual_address) as usize;
            let page_directory_index = get_l2(virtual_address) as usize;
            let page_table_index = get_l1(virtual_address) as usize;

            // Level 4
            let page_map_l4_entry = &mut self.page_map_l4.as_mut().entries[page_map_l4_index];

            if *page_map_l4_entry & flags::PTE_PRESENT == 0 {
                let page_directory_pointer_table = PageDirectoryTablePtr::alloc_new();
                *page_map_l4_entry =
                    (page_directory_pointer_table.to_physical() & ADDR_MASK) | flags::PTE_PRESENT;
            }
            // add new flags if any
            *page_map_l4_entry |= flags;
            eprintln!(
                "L4[{}]: {:p} = {:x}",
                page_map_l4_index, page_map_l4_entry, *page_map_l4_entry
            );

            // Level 3
            let mut page_directory_pointer_table =
                PageDirectoryTablePtr::from_entry(*page_map_l4_entry);

            let page_directory_pointer_entry =
                &mut page_directory_pointer_table.as_mut().entries[page_directory_pointer_index];

            if *page_directory_pointer_entry & flags::PTE_PRESENT == 0 {
                let page_directory_table = PageDirectoryTablePtr::alloc_new();
                *page_directory_pointer_entry =
                    (page_directory_table.to_physical() & ADDR_MASK) | flags::PTE_PRESENT;
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
            let mut page_directory_table =
                PageDirectoryTablePtr::from_entry(*page_directory_pointer_entry);
            let page_directory_entry =
                &mut page_directory_table.as_mut().entries[page_directory_index];

            // here we have an intersection, if we can map a 2MB page, we will, otherwise we will map a 4K page
            // if we are providing the pages (the user didn't provide), then we can't use 2MB pages
            let can_map_2mb_page = physical_address
                .map(|phy_addr| {
                    is_aligned(phy_addr as _, PAGE_2M)
                        && is_aligned(virtual_address as _, PAGE_2M)
                        && size >= PAGE_2M as u64
                })
                .unwrap_or(false);

            if can_map_2mb_page {
                // we already have an entry here
                if *page_directory_entry & flags::PTE_PRESENT != 0 {
                    // did we have a mapping here that lead to 4k pages?
                    // if so, we should free the physical page allocation for them
                    if *page_directory_entry & flags::PTE_HUGE_PAGE == 0 {
                        let page_table_ptr =
                            PageDirectoryTablePtr::from_entry(*page_directory_entry);

                        unsafe { page_table_ptr.free() };
                    }
                }

                // Level 1
                *page_directory_entry = (current_physical_address & ADDR_MASK)
                    | flags
                    | flags::PTE_PRESENT
                    | flags::PTE_HUGE_PAGE;

                eprintln!(
                    "L2[{}] huge: {:p} = {:x}",
                    page_directory_index, page_directory_entry, *page_directory_entry
                );

                size -= PAGE_2M as u64;
                // do not overflow the address
                if size == 0 {
                    break;
                }
                virtual_address += PAGE_2M as u64;
                if let Some(physical_address) = physical_address.as_mut() {
                    *physical_address += PAGE_2M as u64;
                }
            } else {
                // continue mapping 4K pages
                if *page_directory_entry & flags::PTE_PRESENT == 0 {
                    let page_table = PageDirectoryTablePtr::alloc_new();
                    *page_directory_entry =
                        (page_table.to_physical() & ADDR_MASK) | flags::PTE_PRESENT;
                }
                // add new flags
                *page_directory_entry |= flags;
                eprintln!(
                    "L2[{}]: {:p} = {:x}",
                    page_directory_index, page_directory_entry, *page_directory_entry
                );

                // Level 1
                let mut page_table = PageDirectoryTablePtr::from_entry(*page_directory_entry);
                let page_table_entry = &mut page_table.as_mut().entries[page_table_index];
                *page_table_entry =
                    (current_physical_address & ADDR_MASK) | flags | flags::PTE_PRESENT;
                eprintln!(
                    "L1[{}]: {:p} = {:x}",
                    page_table_index, page_table_entry, *page_table_entry
                );

                size -= PAGE_4K as u64;
                // do not overflow the address
                if size == 0 {
                    break;
                }
                virtual_address += PAGE_4K as u64;
                if let Some(physical_address) = physical_address.as_mut() {
                    *physical_address += PAGE_4K as u64;
                }
            }

            eprintln!();
        }
    }

    pub fn is_address_mapped(&self, addr: u64) -> bool {
        let page_map_l4_index = get_l4(addr) as usize;
        let page_directory_pointer_index = get_l3(addr) as usize;
        let page_directory_index = get_l2(addr) as usize;
        let page_table_index = get_l1(addr) as usize;

        // Level 4
        let page_map_l4 = self.page_map_l4.as_ref();
        let page_map_l4_entry = &page_map_l4.entries[page_map_l4_index];

        if *page_map_l4_entry & flags::PTE_PRESENT == 0 {
            return false;
        }
        eprintln!(
            "L4[{}]: {:p} = {:x}",
            page_map_l4_index, page_map_l4_entry, *page_map_l4_entry
        );

        // Level 3
        let page_directory_pointer_table = PageDirectoryTablePtr::from_entry(*page_map_l4_entry);
        let page_directory_pointer_entry =
            &page_directory_pointer_table.as_ref().entries[page_directory_pointer_index];
        if *page_directory_pointer_entry & flags::PTE_PRESENT == 0 {
            return false;
        }
        eprintln!(
            "L3[{}]: {:p} = {:x}",
            page_directory_pointer_index,
            page_directory_pointer_entry,
            *page_directory_pointer_entry
        );

        // Level 2
        let page_directory_table = PageDirectoryTablePtr::from_entry(*page_directory_pointer_entry);
        let page_directory_entry = &page_directory_table.as_ref().entries[page_directory_index];
        if *page_directory_entry & flags::PTE_PRESENT == 0 {
            return false;
        }
        if *page_directory_entry & flags::PTE_HUGE_PAGE != 0 {
            return true;
        }
        eprintln!(
            "L2[{}]: {:p} = {:x}",
            page_directory_index, page_directory_entry, *page_directory_entry
        );

        // Level 1
        let page_table = PageDirectoryTablePtr::from_entry(*page_directory_entry);
        let page_table_entry = &page_table.as_ref().entries[page_table_index];
        if *page_table_entry & flags::PTE_PRESENT == 0 {
            return false;
        }
        eprintln!(
            "L1[{}]: {:p} = {:x}",
            page_table_index, page_table_entry, *page_table_entry
        );

        true
    }

    // the handler function definition is `fn(page_entry: &mut u64)`
    fn do_for_every_user_entry(&mut self, mut f: impl FnMut(&mut u64)) {
        let page_map_l4 = self.page_map_l4.as_mut();

        let present = |entry: &&mut u64| **entry & flags::PTE_PRESENT != 0;

        fn as_page_directory_table_flat(entry: &mut u64) -> IterMut<u64> {
            let page_directory_table = PageDirectoryTablePtr::enteries_from_mut_entry(entry);
            page_directory_table.entries.iter_mut()
        }

        // handle 2MB pages and below
        let handle_2mb_pages = |page_directory_entry: &mut u64| {
            // handle 2MB pages
            if *page_directory_entry & flags::PTE_HUGE_PAGE != 0 {
                f(page_directory_entry);
            } else {
                as_page_directory_table_flat(page_directory_entry)
                    .filter(present)
                    .for_each(&mut f);
            }
        };

        page_map_l4
            .entries
            .iter_mut()
            .take(KERNEL_L4_INDEX) //skip the kernel (the last one)
            .filter(present)
            .flat_map(as_page_directory_table_flat)
            .filter(present)
            .flat_map(as_page_directory_table_flat)
            .filter(present)
            .for_each(handle_2mb_pages);
    }

    // search for all the pages that are mapped to the user ranges and unmap them and free their memory
    pub fn unmap_user_memory(&mut self) {
        let free_page = |entry: &mut u64| {
            assert!(
                *entry & flags::PTE_HUGE_PAGE == 0,
                "We haven't implemented 2MB physical pages for user allocation"
            );
            let page_table_ptr = PageDirectoryTablePtr::from_entry(*entry);
            unsafe { page_table_ptr.free() };
            *entry = 0;
        };

        self.do_for_every_user_entry(free_page);
    }
}
