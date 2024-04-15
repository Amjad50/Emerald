use core::mem;

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BinaryHeap},
    vec::Vec,
};

use crate::{
    cpu::{self, idt::InterruptAllSavedState, interrupts},
    devices::clock::{self, ClockTime},
    memory_management::virtual_memory_mapper,
    process::{syscalls, FxSave},
    sync::spin::mutex::Mutex,
};

use super::{Process, ProcessContext};

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

// an arbitrary value to reset the priority counters
// we don't want to get to 0, as it will result in underflow on subtract
const MIN_PRIORITY_VALUE: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Scheduled,
    WaitingForPid(u64),
    WaitingForTime(ClockTime),
}

/// A wrapper around [`Process`] that has extra details the scheduler cares about
struct SchedulerProcess {
    // using box here so that moving this around won't be as expensive
    process: Box<Process>,
    state: ProcessState,
    priority_counter: u64,
}

impl PartialEq for SchedulerProcess {
    fn eq(&self, other: &Self) -> bool {
        self.priority_counter == other.priority_counter
    }
}

impl PartialOrd for SchedulerProcess {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for SchedulerProcess {}
impl Ord for SchedulerProcess {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.priority_counter.cmp(&other.priority_counter)
    }
}

struct Scheduler {
    interrupt_initialized: bool,
    scheduled_processes: BinaryHeap<SchedulerProcess>,
    running_waiting_procs: BTreeMap<u64, SchedulerProcess>,
    exited_processes: Vec<Process>,
    max_priority: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            interrupt_initialized: false,
            scheduled_processes: BinaryHeap::new(),
            running_waiting_procs: BTreeMap::new(),
            exited_processes: Vec::new(),
            max_priority: u64::MAX,
        }
    }

    pub fn push_process(&mut self, process: Process) {
        // data will be rewritten
        self.reschedule_process(SchedulerProcess {
            process: Box::new(process),
            state: ProcessState::Scheduled,
            priority_counter: self.max_priority,
        })
    }

    fn init_interrupt(&mut self) {
        if self.interrupt_initialized {
            return;
        }
        self.interrupt_initialized = true;

        interrupts::create_scheduler_interrupt(scheduler_interrupt_handler);
        interrupts::create_syscall_interrupt(syscall_interrupt_handler);
    }

    fn reschedule_process(&mut self, mut process: SchedulerProcess) {
        process.priority_counter = self.max_priority;
        process.state = ProcessState::Scheduled;
        self.scheduled_processes.push(process);
    }

    fn reset_scheduled_processes_counters(&mut self) {
        let max_priority = u64::MAX;
        self.scheduled_processes = self
            .scheduled_processes
            .drain()
            .map(|mut p| {
                p.priority_counter = max_priority;
                p
            })
            .collect::<BinaryHeap<_>>();
    }

    fn try_wake_waiting_processes(&mut self) {
        let time_now = clock::clocks().time_since_startup();

        // First, check waiting processes
        let extracted = self
            .running_waiting_procs
            .extract_if(|_, process| {
                let mut remove = false;
                match process.state {
                    ProcessState::WaitingForPid(_) | ProcessState::Running => {
                        self.exited_processes.retain_mut(|exited_proc| {
                            let found_parent = exited_proc.parent_id == process.process.id;

                            // add to parent
                            if found_parent {
                                process
                                    .process
                                    .add_child_exit(exited_proc.id, exited_proc.exit_code);
                            }

                            // wake explicit waiters
                            if let ProcessState::WaitingForPid(pid) = process.state {
                                if pid == exited_proc.id {
                                    remove = true;
                                    // put the exit code in rax
                                    // this should return to user mode directly
                                    assert!(
                                        process.process.context.cs & 0x3 == 3,
                                        "must be from user only"
                                    );
                                    process.process.context.rax = exited_proc.exit_code as u64;
                                }
                            }

                            // retain if we didn't find the parent
                            !found_parent
                        });
                    }
                    ProcessState::WaitingForTime(t) => {
                        if t <= time_now {
                            remove = true;
                        }
                    }
                    _ => unreachable!("We can't have Scheduled state here"),
                }
                remove
            })
            .collect::<Vec<_>>();

        for (_, process) in extracted {
            self.reschedule_process(process);
        }

        // here are processes with parent either in `scheduled_processes` or already gone
        // FIXME: very very inefficient, replace with `Arc` for process in the future
        let mut scheduled_list = self.scheduled_processes.drain().collect::<Vec<_>>();
        for exited_proc in self.exited_processes.drain(..) {
            for process in scheduled_list.iter_mut() {
                if process.process.id == exited_proc.parent_id {
                    process
                        .process
                        .add_child_exit(exited_proc.id, exited_proc.exit_code);
                }
            }
        }
        // put back
        self.scheduled_processes.extend(scheduled_list);

        // we can clear here, since we don't use the vm of the process anymore
        self.exited_processes.clear();
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

        current_cpu.push_cli();

        scheduler.try_wake_waiting_processes();

        // check if we need to reset the priority counters
        if scheduler
            .scheduled_processes
            .peek()
            .map(|p| p.priority_counter < MIN_PRIORITY_VALUE)
            .unwrap_or(false)
        {
            scheduler.reset_scheduled_processes_counters();
        }

        let top = scheduler.scheduled_processes.pop();

        if let Some(mut top) = top {
            assert!(top.state == ProcessState::Scheduled);
            // found a process to run
            top.state = ProcessState::Running;
            // TODO: add priority levels support
            top.priority_counter -= 1;
            scheduler.max_priority = top.priority_counter;
            // SAFETY: we are the scheduler and running in kernel space, so its safe to switch to this vm
            // as it has clones of our kernel mappings
            unsafe { top.process.switch_to_this_vm() };
            current_cpu.process_id = top.process.id;
            current_cpu.context = Some(top.process.context);
            current_cpu.scheduling = true;

            scheduler.running_waiting_procs.insert(top.process.id, top);

            current_cpu.pop_cli();
        }

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

fn with_current_process_and_state<F, U>(f: F) -> U
where
    F: FnOnce(&mut SchedulerProcess) -> U,
{
    let current_cpu = cpu::cpu();
    let mut scheduler = SCHEDULER.lock();
    let process = scheduler
        .running_waiting_procs
        .get_mut(&current_cpu.process_id)
        .expect("current process not found");
    assert!(process.state == ProcessState::Running);
    f(process)
}

/// # Safety
/// Must ensure that this is called and handled inside pop_cli and push_cli block, as an interrupt in the middle
/// causes the `current_process` to be inavailable later on
unsafe fn take_current_process() -> SchedulerProcess {
    let current_cpu = cpu::cpu();
    let process = SCHEDULER
        .lock()
        .running_waiting_procs
        .remove(&current_cpu.process_id)
        .expect("current process not found");
    assert!(process.state == ProcessState::Running);
    process
}

pub fn with_current_process<F, U>(f: F) -> U
where
    F: FnOnce(&mut Process) -> U,
{
    with_current_process_and_state(|p| f(&mut p.process))
}

/// Exit the current process, and move the `all_state` to the scheduler.
/// The caller of this function (i.e. interrupt) will use the `all_state` to go back to the scheduler.
/// This function will remove the context from the CPU, and thus the value in `all_state` will be dropped.
pub fn exit_current_process(exit_code: i32, all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());
    current_cpu.push_cli();

    // SAFETY: called within push_cli and pop_cli
    let mut process = unsafe { take_current_process() };

    eprintln!(
        "Process {} exited with code {}",
        process.process.id, exit_code
    );

    swap_context(current_cpu.context.as_mut().unwrap(), all_state);
    // Even though this context won't run again
    // This may be useful if a process wants to read that context later on.
    // The virtual memory will be cleared once we drop the process
    // thus, we can't drop the process here
    process.process.context = current_cpu.context.take().unwrap();
    process.process.exit(exit_code);

    SCHEDULER.lock().exited_processes.push(*process.process);

    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
}

pub fn sleep_current_process(time: ClockTime, all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    assert!(current_cpu.context.is_some());

    let deadline = clock::clocks().time_since_startup() + time;

    with_current_process_and_state(|p| {
        current_cpu.push_cli();
        p.state = ProcessState::WaitingForTime(deadline);
        eprintln!(
            "Process {} is waiting for time {:?}",
            p.process.id, deadline
        );
        swap_context(current_cpu.context.as_mut().unwrap(), all_state);

        p.process.context = current_cpu.context.take().unwrap();
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
    current_cpu.push_cli();
    // SAFETY: called within push_cli and pop_cli
    let mut process = unsafe { take_current_process() };
    swap_context(current_cpu.context.as_mut().unwrap(), all_state);
    process.process.context = current_cpu.context.take().unwrap();

    SCHEDULER.lock().reschedule_process(process);
    current_cpu.pop_cli();
    // go back to the kernel after the scheduler interrupt
}

pub fn is_process_running(pid: u64) -> bool {
    let scheduler = SCHEDULER.lock();
    scheduler
        .running_waiting_procs
        .values()
        .chain(scheduler.scheduled_processes.iter())
        .any(|p| p.process.id == pid)
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

    with_current_process_and_state(|p| {
        current_cpu.push_cli();
        p.state = ProcessState::WaitingForPid(pid);
        eprintln!("Process {} is waiting for process {}", p.process.id, pid);

        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        p.process.context = current_cpu.context.take().unwrap();
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
