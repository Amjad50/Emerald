pub mod scheduler;

use core::sync::atomic::{AtomicU64, Ordering};

use crate::{
    cpu::{self, gdt},
    executable::{elf, load_elf_to_vm},
    fs,
    memory_management::{
        memory_layout::{KERNEL_BASE, PAGE_4K},
        virtual_memory::{self, VirtualMemoryManager, VirtualMemoryMapEntry},
    },
};

static PROCESS_ID_ALLOCATOR: ProcessIdAllocator = ProcessIdAllocator::new();
const INITIAL_STACK_SIZE_PAGES: usize = 4;

#[derive(Debug)]
pub enum ProcessError {
    CouldNotLoadElf(fs::FileSystemError),
}

impl From<fs::FileSystemError> for ProcessError {
    fn from(e: fs::FileSystemError) -> Self {
        Self::CouldNotLoadElf(e)
    }
}

struct ProcessIdAllocator {
    next_id: AtomicU64,
}

impl ProcessIdAllocator {
    const fn new() -> Self {
        Self {
            next_id: AtomicU64::new(0),
        }
    }

    fn allocate(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[repr(C, align(0x10))]
#[derive(Debug, Clone, Copy, Default)]
pub struct FxSave(pub [u128; 32]);

#[repr(C, align(0x10))]
#[derive(Debug, Clone, Default, Copy)]
pub struct ProcessContext {
    pub rflags: u64,
    pub rip: u64,
    pub cs: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
    pub ss: u64,
    pub dr0: u64,
    pub dr1: u64,
    pub dr2: u64,
    pub dr3: u64,
    pub dr4: u64,
    pub dr5: u64,
    pub dr6: u64,
    pub dr7: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub fxsave: FxSave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Scheduled,
    Sleeping,
    Stopped,
}

// TODO: implement threads, for now each process acts as a thread also
#[allow(dead_code)]
pub struct Process {
    vm: VirtualMemoryManager,
    context: ProcessContext,
    id: u64,
    parent_id: u64,

    stack_ptr_end: usize,
    stack_size: usize,

    state: ProcessState,
}

impl Process {
    pub fn allocate_process(
        parent_id: u64,
        elf: &elf::Elf,
        file: &mut fs::File,
    ) -> Result<Self, ProcessError> {
        let id = PROCESS_ID_ALLOCATOR.allocate();
        let mut vm = virtual_memory::clone_kernel_vm_as_user();
        let stack_end = KERNEL_BASE - PAGE_4K;
        let stack_size = INITIAL_STACK_SIZE_PAGES * PAGE_4K;
        let stack_start = stack_end - stack_size;
        vm.map(&VirtualMemoryMapEntry {
            virtual_address: stack_start as u64,
            physical_address: None,
            size: stack_size as u64,
            flags: virtual_memory::flags::PTE_USER | virtual_memory::flags::PTE_WRITABLE,
        });

        load_elf_to_vm(elf, file, &mut vm)?;

        let mut context = ProcessContext::default();
        let entry = elf.entry_point();
        assert!(vm.is_address_mapped(entry as _) && entry < KERNEL_BASE as u64);

        context.rip = entry;
        context.rsp = stack_end as u64 - 8;
        context.cs = gdt::get_user_code_seg_index().0 | gdt::USER_RING as u64;
        context.ds = gdt::get_user_data_seg_index().0 | gdt::USER_RING as u64;
        context.ss = context.ds;
        context.rflags = cpu::flags::IF;

        Ok(Self {
            vm,
            context,
            id,
            parent_id,
            stack_ptr_end: stack_end - 8, // 8 bytes for padding
            stack_size,
            state: ProcessState::Scheduled,
        })
    }

    pub fn switch_to_this(&mut self) {
        self.vm.switch_to_this();
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn parent_id(&self) -> u64 {
        self.parent_id
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.vm.unmap_user_memory();
    }
}
