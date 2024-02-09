use core::mem;

use alloc::vec::Vec;

use crate::{
    cpu::{self, idt::InterruptAllSavedState, interrupts},
    devices::clock::{self, ClockTime},
    memory_management::virtual_memory_mapper,
    process::{syscalls, FxSave},
    sync::spin::mutex::Mutex,
};

use super::{Process, ProcessContext, ProcessState};

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

struct Scheduler {
    interrupt_initialized: bool,
    processes: Vec<Process>,
    /// There is a process waiting for this time
    earliest_wait: Option<ClockTime>,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            interrupt_initialized: false,
            processes: Vec::new(),
            earliest_wait: None,
        }
    }

    pub fn push_process(&mut self, process: Process) {
        self.processes.push(process);
    }

    fn init_interrupt(&mut self) {
        if self.interrupt_initialized {
            return;
        }
        self.interrupt_initialized = true;

        interrupts::create_scheduler_interrupt(scheduler_interrupt_handler);
        interrupts::create_syscall_interrupt(syscall_interrupt_handler);
    }
}

pub fn push_process(process: Process) {
    SCHEDULER.lock().push_process(process);
}

pub fn schedule() -> ! {
    SCHEDULER.lock().init_interrupt();

    loop {
        let current_cpu = cpu::cpu();
        assert!(current_cpu.context.is_none());

        let mut scheduler = SCHEDULER.lock();
        let earliest_wait = scheduler.earliest_wait.take();
        let time_now = clock::clocks().time_since_startup();
        // we are going to wake one process, so make it a priority
        let going_to_wake = earliest_wait.map(|t| t <= time_now).unwrap_or(false);
        // no context holding, i.e. free to take a new process
        for process in scheduler.processes.iter_mut() {
            let mut run = false;
            match process.state {
                // only schedule another if we don't have current process ready to be run
                ProcessState::Scheduled if current_cpu.context.is_none() && !going_to_wake => {
                    run = true;
                }
                ProcessState::WaitingForTime(t)
                    if current_cpu.context.is_none() && t <= time_now =>
                {
                    run = true;
                }
                ProcessState::Yielded => {
                    // schedule for next time
                    process.state = ProcessState::Scheduled;
                }
                ProcessState::Exited => {
                    // keep the process for one time, it will be deleted later.
                    // this is if we want to do extra stuff later
                }
                _ => {}
            }
            if run {
                // found a process to run
                current_cpu.push_cli();
                process.state = ProcessState::Running;
                // SAFETY: we are the scheduler and running in kernel space, so its safe to switch to this vm
                // as it has clones of our kernel mappings
                unsafe { process.switch_to_this_vm() };
                current_cpu.process_id = process.id;
                current_cpu.context = Some(process.context);
                current_cpu.scheduling = true;
                current_cpu.pop_cli();
            }
        }
        scheduler
            .processes
            .retain(|p| p.state != ProcessState::Exited);
        drop(scheduler);

        if current_cpu.context.is_some() {
            // call scheduler_interrupt_handler
            // we are using interrupts to switch context since it allows us to save the registers of exit, which is
            // very convenient
            // The `sys_exit` syscall changes the context from user to kernel,
            // and because of how we implemented syscalls, the result will be in `rax`, so we tell
            // the compiler to ignore `rax` as it may be clobbered after this call
            unsafe { core::arch::asm!("int 0xff", out("rax") _) }
            // SAFETY: we are not running in any process context, so its safe to go back to the kernel
            unsafe { virtual_memory_mapper::switch_to_kernel() };
        } else {
            // no process to run, just wait for interrupts
            unsafe { cpu::halt() };
        }
    }
}

pub fn with_current_process<F, U>(f: F) -> U
where
    F: FnOnce(&mut Process) -> U,
{
    let current_cpu = cpu::cpu();
    let mut scheduler = SCHEDULER.lock();
    // TODO: find a better way to store processes or store process index/id.
    let process = scheduler
        .processes
        .iter_mut()
        .find(|p| p.id == current_cpu.process_id)
        .expect("current process not found");
    f(process)
}

/// Exit the current process, and move the `all_state` to the scheduler.
/// The caller of this function (i.e. interrupt) will use the `all_state` to go back to the scheduler.
/// This function will remove the context from the CPU, and thus the value in `all_state` will be dropped.
pub fn exit_current_process(exit_code: i32, all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());

    let pid = current_cpu.process_id;
    let mut ppid = 0;
    with_current_process(|process| {
        assert!(process.state == ProcessState::Running);
        current_cpu.push_cli();
        ppid = process.parent_id;
        eprintln!("Process {} exited with code {}", process.id, exit_code);

        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        // clear context from the CPU
        // move the cpu context,
        // this may be useful if a process wants to read that context later on
        // the virtual memory will be cleared once we drop the process
        process.context = current_cpu.context.take().unwrap();
        process.exit(exit_code);
    });
    // notify listeners for this process
    // TODO: do it better with general waiting mechanism not just for pids
    let mut scheduler = SCHEDULER.lock();
    for proc in scheduler.processes.iter_mut() {
        if proc.state == ProcessState::WaitingForPid(pid) {
            // put the exit code in rax
            // this should return to user mode directly
            assert!(proc.context.cs & 0x3 == 3, "must be from user only");
            proc.context.rax = exit_code as u64;
            proc.state = ProcessState::Scheduled;
        }
        if proc.id == ppid {
            proc.add_child_exit(pid, exit_code);
        }
    }

    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
}

pub fn sleep_current_process(time: ClockTime, all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());

    let deadline = clock::clocks().time_since_startup() + time;

    with_current_process(|process| {
        assert!(process.state == ProcessState::Running);
        current_cpu.push_cli();
        process.state = ProcessState::WaitingForTime(deadline);
        eprintln!("Process {} is waiting for time {:?}", process.id, deadline);
        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        // clear context from the CPU
        process.context = current_cpu.context.take().unwrap();
    });
    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
}

pub fn yield_current_if_any(all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    // do not yield if we don't have context or we are in the middle of scheduling
    if current_cpu.context.is_none() || current_cpu.scheduling {
        return;
    }
    // save context of this process and mark is as scheduled
    with_current_process(|process| {
        assert!(process.state == ProcessState::Running);
        current_cpu.push_cli();
        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        // clear context from the CPU
        process.context = current_cpu.context.take().unwrap();
        process.state = ProcessState::Yielded;
    });
    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
}

pub fn is_process_running(pid: u64) -> bool {
    let scheduler = SCHEDULER.lock();
    scheduler.processes.iter().any(|p| p.id == pid)
}

pub fn wait_for_pid(all_state: &mut InterruptAllSavedState, pid: u64) -> bool {
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());

    // we can't wait for a process that doesn't exist now, unless we are a parent of a process that has exited
    // see [`exit_current_process`]
    let process_found = is_process_running(pid);
    if !process_found {
        return false;
    }

    // save context of this process and mark it as waiting
    with_current_process(|process| {
        assert!(process.state == ProcessState::Running);
        current_cpu.push_cli();
        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        // clear context from the CPU
        process.context = current_cpu.context.take().unwrap();
        process.state = ProcessState::WaitingForPid(pid);
    });
    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
    true
}

pub fn swap_context(context: &mut ProcessContext, all_state: &mut InterruptAllSavedState) {
    let mut fxsave = FxSave::default();
    unsafe { core::arch::x86_64::_fxsave64(&mut fxsave as *mut FxSave as _) };
    unsafe { core::arch::x86_64::_fxrstor64(context.fxsave.0.as_ptr() as _) };
    context.fxsave = fxsave;

    mem::swap(&mut all_state.frame.rflags, &mut context.rflags);
    mem::swap(&mut all_state.frame.rip, &mut context.rip);
    all_state.frame.cs = mem::replace(&mut context.cs, all_state.frame.cs as _) as _;
    mem::swap(&mut all_state.frame.rsp, &mut context.rsp);
    all_state.frame.ss = mem::replace(&mut context.ss, all_state.frame.ss as _) as _;

    mem::swap(&mut all_state.rest.ds, &mut context.ds);
    mem::swap(&mut all_state.rest.es, &mut context.es);
    mem::swap(&mut all_state.rest.fs, &mut context.fs);
    mem::swap(&mut all_state.rest.gs, &mut context.gs);
    mem::swap(&mut all_state.rest.dr0, &mut context.dr0);
    mem::swap(&mut all_state.rest.dr1, &mut context.dr1);
    mem::swap(&mut all_state.rest.dr2, &mut context.dr2);
    mem::swap(&mut all_state.rest.dr3, &mut context.dr3);
    mem::swap(&mut all_state.rest.dr6, &mut context.dr6);
    mem::swap(&mut all_state.rest.dr7, &mut context.dr7);
    mem::swap(&mut all_state.rest.rax, &mut context.rax);
    mem::swap(&mut all_state.rest.rbx, &mut context.rbx);
    mem::swap(&mut all_state.rest.rcx, &mut context.rcx);
    mem::swap(&mut all_state.rest.rdx, &mut context.rdx);
    mem::swap(&mut all_state.rest.rsi, &mut context.rsi);
    mem::swap(&mut all_state.rest.rdi, &mut context.rdi);
    mem::swap(&mut all_state.rest.rbp, &mut context.rbp);
    mem::swap(&mut all_state.rest.r8, &mut context.r8);
    mem::swap(&mut all_state.rest.r9, &mut context.r9);
    mem::swap(&mut all_state.rest.r10, &mut context.r10);
    mem::swap(&mut all_state.rest.r11, &mut context.r11);
    mem::swap(&mut all_state.rest.r12, &mut context.r12);
    mem::swap(&mut all_state.rest.r13, &mut context.r13);
    mem::swap(&mut all_state.rest.r14, &mut context.r14);
    mem::swap(&mut all_state.rest.r15, &mut context.r15);
}

extern "cdecl" fn scheduler_interrupt_handler(all_state: &mut InterruptAllSavedState) {
    assert!(all_state.frame.cs & 0x3 == 0, "must be from kernel only");
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());
    assert!(current_cpu.scheduling);
    assert!(current_cpu.interrupts_disabled());

    // we can yield at this point after we go to the process
    current_cpu.scheduling = false;

    swap_context(current_cpu.context.as_mut().unwrap(), all_state);
}

extern "cdecl" fn syscall_interrupt_handler(all_state: &mut InterruptAllSavedState) {
    assert!(all_state.frame.cs & 0x3 == 3, "must be from user only");
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());

    syscalls::handle_syscall(all_state);
}
