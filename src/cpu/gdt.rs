use core::mem::{self, size_of};

use crate::{
    memory_management::{
        memory_layout::{
            is_aligned, virtual2physical, INTR_STACK_BASE, INTR_STACK_EMPTY_SIZE,
            INTR_STACK_ENTRY_SIZE, INTR_STACK_SIZE, INTR_STACK_TOTAL_SIZE, PAGE_4K,
        },
        physical_page_allocator,
        virtual_memory::{self, VirtualMemoryMapEntry},
    },
    sync::spin::mutex::Mutex,
};

static GDT: Mutex<GlobalDescriptorManager> = Mutex::new(GlobalDescriptorManager::empty());
/// SAFETY: TSS is only used when `GDT` is locked, so its safe to use as `static mut`
static mut TSS: TaskStateSegment = TaskStateSegment::empty();

const KERNEL_RING: u8 = 0;
const USER_RING: u8 = 3;

/// This should be called only once, otherwise, it will crash
pub fn init_kernel_gdt() {
    let mut manager = GDT.lock();
    if manager.gdt.index != 1 {
        panic!("GDT already initialized");
    }

    manager.kernel_code_seg_index = unsafe {
        manager.gdt.push_user(UserDescriptorEntry {
            access: flags::PRESENT | flags::CODE | flags::USER | flags::dpl(KERNEL_RING),
            flags_and_limit: flags::LONG_MODE,
            ..UserDescriptorEntry::empty()
        })
    };
    manager.data_seg_index = unsafe {
        manager.gdt.push_user(UserDescriptorEntry {
            access: flags::PRESENT | flags::USER,
            ..UserDescriptorEntry::empty()
        })
    };
    manager.user_code_seg_index = unsafe {
        manager.gdt.push_user(UserDescriptorEntry {
            access: flags::PRESENT | flags::CODE | flags::USER | flags::dpl(USER_RING),
            flags_and_limit: flags::LONG_MODE,
            ..UserDescriptorEntry::empty()
        })
    };

    // setup TSS

    // setup stacks, for each use `INTR_STACK_SIZE` bytes, but also allocate another one of these
    // and use as padding between the stacks, so that we can detect stack overflows
    for i in 0..7 {
        unsafe {
            // allocate after an empty offset, so that we can detect stack overflows
            let mut stack_start_virtual =
                INTR_STACK_BASE + (i * INTR_STACK_ENTRY_SIZE) + INTR_STACK_EMPTY_SIZE;
            let stack_end_virtual = stack_start_virtual + INTR_STACK_SIZE;
            assert!(stack_end_virtual <= INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE);
            if i == 6 {
                // make sure we have allocated everything
                assert!(stack_end_virtual == INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE);
            }
            // make sure that the stack is aligned, so we can easily allocate pages
            assert!(
                is_aligned(INTR_STACK_SIZE as _, PAGE_4K)
                    && is_aligned(stack_start_virtual as _, PAGE_4K)
            );

            // allocate pages and map them
            // TODO: add a new dynamic virtual allocator and replace this
            while stack_start_virtual < stack_end_virtual {
                let stack_start_phy =
                    virtual2physical(physical_page_allocator::alloc_zeroed() as _);
                virtual_memory::map(&VirtualMemoryMapEntry {
                    virtual_address: stack_start_virtual as u64,
                    start_physical_address: stack_start_phy as u64,
                    end_physical_address: (stack_start_phy + PAGE_4K) as u64,
                    flags: virtual_memory::flags::PTE_WRITABLE,
                });
                stack_start_virtual += PAGE_4K;
            }
            // set the stack pointer
            // subtract 8, since the boundary is not mapped
            TSS.ist[i] = stack_end_virtual as u64 - 8;
        }
    }

    let tss_ptr = (unsafe { &TSS } as *const _) as u64;

    manager.tss_seg_index = unsafe {
        manager.gdt.push_system(SystemDescriptorEntry {
            limit: (mem::size_of::<TaskStateSegment>() - 1) as u16,
            access: flags::PRESENT | flags::TSS_TYPE,
            base_low: (tss_ptr & 0xFFFF) as u16,
            base_middle: ((tss_ptr >> 16) & 0xFF) as u8,
            base_high: ((tss_ptr >> 24) & 0xFF) as u8,
            base_upper: ((tss_ptr >> 32) & 0xFFFFFFFF) as u32,
            ..SystemDescriptorEntry::empty()
        })
    };
    drop(manager);
    // call the special `run_with` so that we get the `static` lifetime
    GDT.run_with(|manager| {
        manager.gdt.apply_lgdt();

        manager.load_kernel_segments();
        manager.load_tss();
    });
}

mod flags {
    // this is in the flags byte
    pub const LONG_MODE: u8 = 1 << 5;

    // these are in the access byte
    pub const PRESENT: u8 = 1 << 7;
    pub const CODE: u8 = 1 << 3;
    pub const USER: u8 = 1 << 4;
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
struct TaskStateSegment {
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

struct GlobalDescriptorManager {
    gdt: GlobalDescriptorTable,
    kernel_code_seg_index: usize,
    user_code_seg_index: usize,
    // there is only one data segment and its not even used, as we are using
    // segments 0 for ds, ss, es, and others.
    data_seg_index: usize,
    tss_seg_index: usize,
}

impl GlobalDescriptorManager {
    pub const fn empty() -> Self {
        Self {
            gdt: GlobalDescriptorTable::empty(),
            kernel_code_seg_index: 0,
            data_seg_index: 0,
            user_code_seg_index: 0,
            tss_seg_index: 0,
        }
    }

    pub fn load_kernel_segments(&self) {
        assert!(self.kernel_code_seg_index != 0);
        unsafe {
            // load the code segment
            super::set_cs((self.kernel_code_seg_index * size_of::<u64>()) as u16);
        }
    }

    pub fn load_tss(&self) {
        assert!(self.tss_seg_index != 0);
        unsafe {
            // load the tss segment
            super::ltr((self.tss_seg_index * size_of::<u64>()) as u16);
        }
    }
}

#[repr(C, packed(16))]
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
        assert!(mem::size_of::<UserDescriptorEntry>() == 8);
        let index = self.index;
        self.index += 1;
        // SAFETY: This is valid because its 8 bytes and
        self.data[index] = core::mem::transmute::<_, u64>(entry);
        index
    }

    /// Must make sure that the data is a valid descriptor following the spec
    unsafe fn push_system(&mut self, entry: SystemDescriptorEntry) -> usize {
        assert!(mem::size_of::<SystemDescriptorEntry>() == 16);
        // SAFETY: This is valid because its 16 bytes and
        let data = core::mem::transmute::<_, [u64; 2]>(entry);
        let index = self.index;
        self.index += 2;
        self.data[index] = data[0];
        self.data[index + 1] = data[1];
        index
    }

    pub fn apply_lgdt(&'static self) {
        let size_used = self.index * mem::size_of::<u64>() - 1;
        let gdt_ptr = GlobalDescriptorTablePointer {
            limit: size_used as u16,
            base: self,
        };

        unsafe {
            super::lgdt(&gdt_ptr);
        }
    }
}
