pub mod scheduler;
mod syscalls;

use core::sync::atomic::{AtomicU64, Ordering};

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{
    cpu::{self, gdt},
    executable::{elf, load_elf_to_vm},
    fs,
    memory_management::{
        memory_layout::{align_up, is_aligned, GB, KERNEL_BASE, MB, PAGE_2M, PAGE_4K},
        virtual_memory_mapper::{
            self, VirtualMemoryMapEntry, VirtualMemoryMapper, MAX_USER_VIRTUAL_ADDRESS,
        },
    },
};

static PROCESS_ID_ALLOCATOR: GoingUpAllocator = GoingUpAllocator::new();
const INITIAL_STACK_SIZE_PAGES: usize = 4;

#[allow(clippy::identity_op)]
const HEAP_OFFSET_FROM_ELF_END: usize = 1 * MB;
#[allow(clippy::identity_op)]
const DEAFULT_MAX_HEAP_SIZE: usize = 1 * GB;

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
    WaitingForPid(u64),
}

// TODO: implement threads, for now each process acts as a thread also
#[allow(dead_code)]
pub struct Process {
    vm: VirtualMemoryMapper,
    context: ProcessContext,
    id: u64,
    parent_id: u64,

    // use BTreeMap to keep FDs even after closing some of them
    open_filesystem_nodes: BTreeMap<usize, fs::FilesystemNode>,
    file_index_allocator: GoingUpAllocator,

    argv: Vec<String>,

    stack_ptr_end: usize,
    stack_size: usize,

    heap_start: usize,
    heap_size: usize,
    heap_max: usize,

    state: ProcessState,
    // split from the state, so that we can keep it as a simple enum
    exit_code: i32,
    children_exits: BTreeMap<u64, i32>,
}

impl Process {
    pub fn allocate_process(
        parent_id: u64,
        elf: &elf::Elf,
        file: &mut fs::File,
        argv: Vec<String>,
    ) -> Result<Self, ProcessError> {
        let id = PROCESS_ID_ALLOCATOR.allocate();
        let mut vm = virtual_memory_mapper::clone_current_vm_as_user();
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

        let rsp = stack_end as u64 - 8;
        let (new_rsp, argc, argv_ptr) =
            Self::load_argv_into_stack(&mut vm, &argv, rsp, stack_start as u64);

        // SAFETY: we know that the vm passed is an exact kernel copy of this vm, so its safe to switch to it
        // TODO: maybe it would be best to create the new vm inside this function?
        let (_min_addr, max_addr) = unsafe { load_elf_to_vm(elf, file, &mut vm)? };
        // SAFETY: we know that the vm is never used after this point until scheduling
        unsafe { vm.add_process_specific_mappings() };

        // set it quite a distance from the elf and align it to 2MB pages (we are not using 2MB virtual memory, so its not related)
        let heap_start = align_up(max_addr + HEAP_OFFSET_FROM_ELF_END, PAGE_2M);
        let heap_size = 0; // start at 0, let user space programs control it
        let heap_max = DEAFULT_MAX_HEAP_SIZE;

        let mut context = ProcessContext::default();
        let entry = elf.entry_point();
        assert!(vm.is_address_mapped(entry as _) && entry < KERNEL_BASE as u64);

        context.rip = entry;
        context.cs = gdt::get_user_code_seg_index().0 | gdt::USER_RING as u64;
        context.ds = gdt::get_user_data_seg_index().0 | gdt::USER_RING as u64;
        context.ss = context.ds;
        context.rflags = cpu::flags::IF;

        // setup main function arguments and stack
        context.rsp = new_rsp;
        // NOTE: This is very specific to x86_64 SYSV abi
        context.rdi = argc;
        context.rsi = argv_ptr;

        Ok(Self {
            vm,
            context,
            id,
            parent_id,
            open_filesystem_nodes: BTreeMap::new(),
            file_index_allocator: GoingUpAllocator::new(),
            argv,
            stack_ptr_end: stack_end - 8, // 8 bytes for padding
            stack_size,
            heap_start,
            heap_size,
            heap_max,
            state: ProcessState::Scheduled,
            exit_code: 0,
            children_exits: BTreeMap::new(),
        })
    }

    /// # Safety
    /// Check [`virtual_memory_mapper::VirtualMemoryMapper::switch_to_this`] for more info
    pub unsafe fn switch_to_this_vm(&mut self) {
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

    pub fn finish_stdio(&mut self) {
        // make sure we have STDIN/STDOUT/STDERR, and the allocator is after them
        assert!(self.open_filesystem_nodes.len() >= 3);
        if self.file_index_allocator.next_id.load(Ordering::Relaxed) < 3 {
            self.file_index_allocator
                .next_id
                .store(3, Ordering::Relaxed);
        }
    }

    pub fn push_fs_node<F: Into<fs::FilesystemNode>>(&mut self, file: F) -> usize {
        let fd = self.file_index_allocator.allocate() as usize;
        assert!(
            self.open_filesystem_nodes.insert(fd, file.into()).is_none(),
            "fd already exists"
        );
        fd
    }

    pub fn attach_fs_node_to_fd<F: Into<fs::FilesystemNode>>(
        &mut self,
        fd: usize,
        file: F,
    ) -> bool {
        // fail first
        if self.open_filesystem_nodes.contains_key(&fd) {
            return false;
        }
        // update allocator so that next push_file will not overwrite this fd
        self.file_index_allocator
            .next_id
            .store(fd as u64 + 1, Ordering::SeqCst);
        // must always return `true`
        self.open_filesystem_nodes.insert(fd, file.into()).is_none()
    }

    pub fn get_fs_node(&mut self, fd: usize) -> Option<&mut fs::FilesystemNode> {
        self.open_filesystem_nodes.get_mut(&fd)
    }

    pub fn take_fs_node(&mut self, fd: usize) -> Option<fs::FilesystemNode> {
        self.open_filesystem_nodes.remove(&fd)
    }

    pub fn put_fs_node(&mut self, fd: usize, file: fs::FilesystemNode) {
        assert!(
            self.open_filesystem_nodes.insert(fd, file).is_none(),
            "fd already exists"
        )
    }

    pub fn exit(&mut self, exit_code: i32) {
        self.state = ProcessState::Exited;
        self.exit_code = exit_code;
    }

    pub fn add_child_exit(&mut self, pid: u64, exit_code: i32) {
        assert!(
            self.children_exits.insert(pid, exit_code).is_none(),
            "child pid already exists"
        );
    }

    pub fn get_child_exit(&mut self, pid: u64) -> Option<i32> {
        self.children_exits.remove(&pid)
    }

    /// Add/Remove to/from the heap and return the previous end of the heap before the change
    /// If this is an `Add`, it will return the address of the new block
    /// If this is a `Remove`, the result will generally be useless
    /// Use with `0` to get the current heap end
    pub fn add_to_heap(&mut self, increment: isize) -> Option<usize> {
        if increment == 0 {
            return Some(self.heap_start + self.heap_size);
        }

        assert!(is_aligned(increment.unsigned_abs(), PAGE_4K));

        let new_size = self.heap_size as isize + increment;
        if new_size < 0 || new_size as usize > self.heap_max {
            return None;
        }
        let old_end = self.heap_start + self.heap_size;
        self.heap_size = new_size as usize;
        if increment > 0 {
            // map the new heap
            let entry = VirtualMemoryMapEntry {
                virtual_address: old_end as u64,
                physical_address: None,
                size: increment as u64,
                flags: virtual_memory_mapper::flags::PTE_USER
                    | virtual_memory_mapper::flags::PTE_WRITABLE,
            };
            self.vm.map(&entry);
        } else {
            let new_end = old_end - increment.unsigned_abs();
            // unmap old heap
            let entry = VirtualMemoryMapEntry {
                virtual_address: new_end as u64,
                physical_address: None,
                size: increment.unsigned_abs() as u64,
                flags: virtual_memory_mapper::flags::PTE_USER
                    | virtual_memory_mapper::flags::PTE_WRITABLE,
            };
            // `true` because we allocated physical memory using `map`
            self.vm.unmap(&entry, true);
        }

        Some(old_end)
    }
}

impl Process {
    // NOTE: this is very specific to 64bit x86
    fn load_argv_into_stack(
        vm: &mut VirtualMemoryMapper,
        argv: &[String],
        mut rsp: u64,
        stack_top: u64,
    ) -> (u64, u64, u64) {
        // dealing with vm, so we must disable interrupts
        cpu::cpu().push_cli();
        let old_vm = virtual_memory_mapper::get_current_vm();

        // switch temporaily so we can map the elf
        // SAFETY: this must be called while the current vm and this new vm must share the same
        //         kernel regions
        unsafe { vm.switch_to_this() };

        let argc = argv.len();

        let mut argv_ptrs = Vec::with_capacity(argv.len());
        for arg in argv.iter() {
            let arg_ptr = rsp - arg.len() as u64 - 1;
            rsp = arg_ptr;
            // align to 8 bytes
            rsp -= rsp % 8;
            assert!(rsp >= stack_top);

            // convert arg_ptr to slice
            let arg_ptr_slice =
                unsafe { core::slice::from_raw_parts_mut(arg_ptr as *mut u8, arg.len() + 1) };
            // copy the arg
            arg_ptr_slice[..arg.len()].copy_from_slice(arg.as_bytes());
            // put null terminator
            arg_ptr_slice[arg.len()] = 0;

            argv_ptrs.push(arg_ptr);
        }
        // align to 8 bytes
        rsp -= rsp % 8;
        assert!(rsp >= stack_top);
        // add null terminator
        let null_ptr = rsp - 1;
        rsp = null_ptr;
        unsafe { (null_ptr as *mut u8).write(0) };
        argv_ptrs.push(null_ptr);
        // align to 8 bytes
        rsp -= rsp % 8;
        assert!(rsp >= stack_top);

        // write the argv array
        let argv_array_ptr = rsp - (argv_ptrs.len() * 8) as u64;
        rsp = argv_array_ptr;
        let argv_array_ptr_slice =
            unsafe { core::slice::from_raw_parts_mut(argv_array_ptr as *mut u64, argv_ptrs.len()) };
        argv_array_ptr_slice.copy_from_slice(&argv_ptrs);

        // these are not needed really, since in x86_64 we are using the registers to pass arguments
        // but we can keep it for the future
        // add pointer to argv array
        rsp -= 8;
        assert!(rsp >= stack_top);
        unsafe { (rsp as *mut u64).write(argv_array_ptr) };
        // add argc
        rsp -= 8;
        assert!(rsp >= stack_top);
        unsafe { (rsp as *mut u64).write(argc as u64) };

        // switch back to the old vm
        unsafe { old_vm.switch_to_this() };
        // we can be interrupted again
        cpu::cpu().pop_cli();

        (rsp, argc as u64, argv_array_ptr)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.vm.unmap_process_memory();
    }
}
