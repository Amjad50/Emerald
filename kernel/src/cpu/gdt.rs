use core::{mem, pin::Pin, ptr::addr_of};

use crate::{
    cpu,
    memory_management::{
        memory_layout::{
            is_aligned, INTR_STACK_BASE, INTR_STACK_EMPTY_SIZE, INTR_STACK_ENTRY_SIZE,
            INTR_STACK_SIZE, INTR_STACK_TOTAL_SIZE, PAGE_4K, PROCESS_KERNEL_STACK_END,
        },
        virtual_memory_mapper::{self, VirtualMemoryMapEntry},
    },
};

pub const KERNEL_RING: u8 = 0;
pub const USER_RING: u8 = 3;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SegmentSelector(pub u64);

impl SegmentSelector {
    pub const fn from_index(index: usize) -> Self {
        assert!(index < 8192);
        Self((index << 3) as u64)
    }
}

pub fn get_user_code_seg_index() -> SegmentSelector {
    // TODO: for now, we use the segment from the current CPU
    //       technically, its all would be the same
    cpu::cpu().gdt.user_code_seg
}

pub fn get_user_data_seg_index() -> SegmentSelector {
    // TODO: for now, we use the segment from the current CPU
    //       technically, its all would be the same
    cpu::cpu().gdt.user_data_seg
}

mod flags {
    // this is in the flags byte
    pub const LONG_MODE: u8 = 1 << 5;

    // these are in the access byte
    pub const PRESENT: u8 = 1 << 7;
    pub const CODE: u8 = 1 << 3;
    pub const USER: u8 = 1 << 4;
    pub const WRITE: u8 = 1 << 1;
    pub const TSS_TYPE: u8 = 0b1001;
    pub const fn dpl(dpl: u8) -> u8 {
        dpl << 5
    }
}

/// User Descriptor Entry for GDT
///
/// This includes only `code` and `data` descriptors in 64-bit mode
#[repr(C, packed(4))]
#[derive(Default, Clone, Copy)]
struct UserDescriptorEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    flags_and_limit: u8,
    base_high: u8,
}

impl UserDescriptorEntry {
    pub const fn empty() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            flags_and_limit: 0,
            base_high: 0,
        }
    }
}

/// System Descriptor Entry for GDT
///
/// This includes only `TSS` descriptors in 64-bit mode
#[repr(C, packed(4))]
#[derive(Default, Clone, Copy)]
struct SystemDescriptorEntry {
    limit: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    flags_and_limit: u8,
    base_high: u8,
    base_upper: u32,
    zero: u32,
}

impl SystemDescriptorEntry {
    pub const fn empty() -> Self {
        Self {
            limit: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            flags_and_limit: 0,
            base_high: 0,
            base_upper: 0,
            zero: 0,
        }
    }
}

/// Task State Segment
///
/// This is the structure that is pointed to by the `TSS` descriptor
#[repr(C, packed(4))]
#[derive(Debug, Clone, Copy)]
pub struct TaskStateSegment {
    reserved: u32,
    rsp: [u64; 3],
    reserved2: u64,
    ist: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn empty() -> Self {
        Self {
            reserved: 0,
            rsp: [0; 3],
            reserved2: 0,
            ist: [0; 7],
            reserved3: 0,
            reserved4: 0,
            iomap_base: 0,
        }
    }
}

#[repr(C, packed(2))]
pub(super) struct GlobalDescriptorTablePointer {
    limit: u16,
    base: *const GlobalDescriptorTable,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalDescriptorManager {
    gdt: GlobalDescriptorTable,
    tss: TaskStateSegment,
    kernel_code_seg: SegmentSelector,
    user_code_seg: SegmentSelector,
    // there is only one data segment and its not even used, as we are using
    // segments 0 for ds, ss, es, and others.
    kernel_data_seg: SegmentSelector,
    user_data_seg: SegmentSelector,
    tss_seg: SegmentSelector,
}

impl GlobalDescriptorManager {
    pub const fn empty() -> Self {
        Self {
            gdt: GlobalDescriptorTable::empty(),
            tss: TaskStateSegment::empty(),
            kernel_code_seg: SegmentSelector::from_index(0),
            kernel_data_seg: SegmentSelector::from_index(0),
            user_code_seg: SegmentSelector::from_index(0),
            user_data_seg: SegmentSelector::from_index(0),
            tss_seg: SegmentSelector::from_index(0),
        }
    }

    fn gdt(self: Pin<&Self>) -> Pin<&GlobalDescriptorTable> {
        // SAFETY: This is safe because we are using `Pin<&Self>` which guarantees that
        // the GDT is not moved, and we are accessing it in a read-only manner.
        unsafe { self.map_unchecked(|s| &s.gdt) }
    }

    pub fn init_segments(mut self: Pin<&'static mut Self>) {
        if self.gdt.index != 1 {
            panic!("GDT already initialized");
        }

        self.kernel_code_seg = SegmentSelector::from_index(unsafe {
            self.gdt.push_user(UserDescriptorEntry {
                access: flags::PRESENT | flags::CODE | flags::USER | flags::dpl(KERNEL_RING),
                flags_and_limit: flags::LONG_MODE,
                ..UserDescriptorEntry::empty()
            })
        });
        self.user_code_seg = SegmentSelector::from_index(unsafe {
            self.gdt.push_user(UserDescriptorEntry {
                access: flags::PRESENT | flags::CODE | flags::USER | flags::dpl(USER_RING),
                flags_and_limit: flags::LONG_MODE,
                ..UserDescriptorEntry::empty()
            })
        });
        self.kernel_data_seg = SegmentSelector::from_index(unsafe {
            self.gdt.push_user(UserDescriptorEntry {
                access: flags::PRESENT | flags::USER | flags::WRITE | flags::dpl(KERNEL_RING),
                ..UserDescriptorEntry::empty()
            })
        });
        self.user_data_seg = SegmentSelector::from_index(unsafe {
            self.gdt.push_user(UserDescriptorEntry {
                access: flags::PRESENT | flags::USER | flags::WRITE | flags::dpl(USER_RING),
                ..UserDescriptorEntry::empty()
            })
        });

        // setup TSS

        // setup stacks, for each use `INTR_STACK_SIZE` bytes, but also allocate another one of these
        // and use as padding between the stacks, so that we can detect stack overflows
        for i in 0..7 {
            // allocate after an empty offset, so that we can detect stack overflows
            let stack_start_virtual =
                INTR_STACK_BASE + (i * INTR_STACK_ENTRY_SIZE) + INTR_STACK_EMPTY_SIZE;
            let stack_end_virtual = stack_start_virtual + INTR_STACK_SIZE;
            assert!(stack_end_virtual <= INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE);
            if i == 6 {
                // make sure we have allocated everything
                assert_eq!(stack_end_virtual, INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE);
            }
            // make sure that the stack is aligned, so we can easily allocate pages
            assert!(
                is_aligned(INTR_STACK_SIZE, PAGE_4K) && is_aligned(stack_start_virtual, PAGE_4K)
            );

            // map the stack
            virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
                virtual_address: stack_start_virtual,
                physical_address: None,
                size: INTR_STACK_SIZE,
                flags: virtual_memory_mapper::flags::PTE_WRITABLE,
            });

            // set the stack pointer
            // subtract 8, since the boundary is not mapped
            self.tss.ist[i] = stack_end_virtual as u64 - 8;

            // A kernel stack for this process
            // this will be used on transitions from user to kernel
            self.tss.rsp[KERNEL_RING as usize] = PROCESS_KERNEL_STACK_END as u64 - 8;
        }

        let tss_ptr = addr_of!(self.tss) as u64;

        self.tss_seg = SegmentSelector::from_index(unsafe {
            self.gdt.push_system(SystemDescriptorEntry {
                limit: (mem::size_of::<TaskStateSegment>() - 1) as u16,
                access: flags::PRESENT | flags::TSS_TYPE,
                base_low: (tss_ptr & 0xFFFF) as u16,
                base_middle: ((tss_ptr >> 16) & 0xFF) as u8,
                base_high: ((tss_ptr >> 24) & 0xFF) as u8,
                base_upper: ((tss_ptr >> 32) & 0xFFFFFFFF) as u32,
                ..SystemDescriptorEntry::empty()
            })
        });
        // convert to ref at this point and do the actual loading
        let s = self.into_ref();
        s.gdt().apply_lgdt(); // apply the GDT
        s.load_kernel_segments();
        s.load_tss();
    }

    pub fn load_kernel_segments(&self) {
        assert_ne!(self.kernel_code_seg.0, 0);
        unsafe {
            // load the code segment
            super::set_cs(self.kernel_code_seg);
            // load the data segments
            super::set_data_segments(self.kernel_data_seg);
        }
    }

    pub fn load_tss(&self) {
        assert_ne!(self.tss_seg.0, 0);
        unsafe {
            // load the tss segment
            super::ltr(self.tss_seg);
        }
    }
}

#[repr(C, packed(16))]
#[derive(Debug, Clone, Copy)]
struct GlobalDescriptorTable {
    data: [u64; 8],
    index: usize,
}

impl GlobalDescriptorTable {
    const fn empty() -> Self {
        Self {
            data: [0; 8],
            index: 1,
        }
    }

    /// Must make sure that the data is a valid descriptor following the spec
    unsafe fn push_user(&mut self, entry: UserDescriptorEntry) -> usize {
        assert_eq!(mem::size_of::<UserDescriptorEntry>(), 8);
        let index = self.index;
        self.index += 1;
        // SAFETY: This is valid because its 8 bytes and
        self.data[index] = core::mem::transmute::<UserDescriptorEntry, u64>(entry);
        index
    }

    /// Must make sure that the data is a valid descriptor following the spec
    unsafe fn push_system(&mut self, entry: SystemDescriptorEntry) -> usize {
        assert_eq!(mem::size_of::<SystemDescriptorEntry>(), 16);
        // SAFETY: This is valid because its 16 bytes and
        let data = core::mem::transmute::<SystemDescriptorEntry, [u64; 2]>(entry);
        let index = self.index;
        self.index += 2;
        self.data[index] = data[0];
        self.data[index + 1] = data[1];
        index
    }

    pub fn apply_lgdt(self: Pin<&'static Self>) {
        let size_used = self.index * mem::size_of::<u64>() - 1;
        let base: &'static Self = self.get_ref();
        let gdt_ptr = GlobalDescriptorTablePointer {
            limit: size_used as u16,
            base,
        };

        unsafe {
            super::lgdt(&gdt_ptr);
        }
    }
}
