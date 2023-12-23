pub mod scheduler;
mod syscalls;

use core::sync::atomic::{AtomicU64, Ordering};

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{
    cpu::{self, gdt},
    executable::{elf, load_elf_to_vm},
    fs,
    memory_management::{
        memory_layout::{KERNEL_BASE, PAGE_4K},
        virtual_memory_mapper::{
            self, VirtualMemoryMapEntry, VirtualMemoryMapper, MAX_USER_VIRTUAL_ADDRESS,
        },
    },
};

static PROCESS_ID_ALLOCATOR: GoingUpAllocator = GoingUpAllocator::new();
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

struct GoingUpAllocator {
    next_id: AtomicU64,
}

impl GoingUpAllocator {
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
    Yielded, // Not used now, but should be scheduled next
    Scheduled,
    Sleeping,
    Exited,
}

// TODO: implement threads, for now each process acts as a thread also
#[allow(dead_code)]
pub struct Process {
    vm: VirtualMemoryMapper,
    context: ProcessContext,
    id: u64,
    parent_id: u64,

    // use BTreeMap to keep FDs even after closing some of them
    open_files: BTreeMap<usize, fs::File>,
    file_index_allocator: GoingUpAllocator,

    argv: Vec<String>,

    stack_ptr_end: usize,
    stack_size: usize,

    state: ProcessState,
    // split from the state, so that we can keep it as a simple enum
    exit_code: u64,
}

impl Process {
    pub fn allocate_process(
        parent_id: u64,
        elf: &elf::Elf,
        file: &mut fs::File,
        argv: Vec<String>,
    ) -> Result<Self, ProcessError> {
        let id = PROCESS_ID_ALLOCATOR.allocate();
        let mut vm = virtual_memory_mapper::clone_kernel_vm_as_user();
        let stack_end = MAX_USER_VIRTUAL_ADDRESS - PAGE_4K;
        let stack_size = INITIAL_STACK_SIZE_PAGES * PAGE_4K;
        let stack_start = stack_end - stack_size;
        vm.map(&VirtualMemoryMapEntry {
            virtual_address: stack_start as u64,
            physical_address: None,
            size: stack_size as u64,
            flags: virtual_memory_mapper::flags::PTE_USER
                | virtual_memory_mapper::flags::PTE_WRITABLE,
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
            open_files: BTreeMap::new(),
            file_index_allocator: GoingUpAllocator::new(),
            argv,
            stack_ptr_end: stack_end - 8, // 8 bytes for padding
            stack_size,
            state: ProcessState::Scheduled,
            exit_code: 0,
        })
    }

    pub fn switch_to_this_vm(&mut self) {
        self.vm.switch_to_this();
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn parent_id(&self) -> u64 {
        self.parent_id
    }

    pub fn is_user_address_mapped(&self, address: u64) -> bool {
        self.vm.is_address_mapped(address)
    }

    pub fn push_file(&mut self, file: fs::File) -> usize {
        let fd = self.file_index_allocator.allocate() as usize;
        assert!(
            self.open_files.insert(fd, file).is_none(),
            "fd already exists"
        );
        fd
    }

    pub fn attach_file_to_fd(&mut self, fd: usize, file: fs::File) -> bool {
        // fail first
        if self.open_files.contains_key(&fd) {
            return false;
        }
        // update allocator so that next push_file will not overwrite this fd
        self.file_index_allocator
            .next_id
            .store(fd as u64 + 1, Ordering::SeqCst);
        // must always return `true`
        self.open_files.insert(fd, file).is_none()
    }

    pub fn get_file(&mut self, fd: usize) -> Option<&mut fs::File> {
        self.open_files.get_mut(&fd)
    }

    pub fn exit(&mut self, exit_code: u64) {
        self.state = ProcessState::Exited;
        self.exit_code = exit_code;
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.vm.unmap_user_memory();
    }
}
