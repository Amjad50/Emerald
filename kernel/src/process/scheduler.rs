use core::{hint, mem};

use crate::{
    cpu::{self, idt::InterruptAllSavedState, interrupts},
    memory_management::virtual_memory,
    process::FxSave,
    sync::spin::mutex::Mutex,
};

use super::{ProcessContext, ProcessState, PROCESSES};

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

struct Scheduler {
    interrupt_initialized: bool,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            interrupt_initialized: false,
        }
    }

    fn init_interrupt(&mut self) {
        if self.interrupt_initialized {
            return;
        }
        self.interrupt_initialized = true;

        interrupts::create_scheduler_interrupt(scheduler_interrupt_handler);
    }
}

pub fn schedule() -> ! {
    SCHEDULER.lock().init_interrupt();

    loop {
        let current_cpu = cpu::cpu();
        if current_cpu.context.is_some() {
            loop {
                hint::spin_loop();
            }
        }

        // no context holding, i.e. free to take a new process
        let mut processes = PROCESSES.lock();
        for process in processes.iter_mut() {
            if process.state == ProcessState::Scheduled {
                current_cpu.push_cli();
                process.state = ProcessState::Running;
                process.switch_to_this();
                current_cpu.process_id = process.id;
                current_cpu.context = Some(process.context);

                current_cpu.pop_cli();
            }
        }
        drop(processes);

        if current_cpu.context.is_some() {
            // call scheduler_interrupt_handler
            // we are using interrupts to switch context since it allows us to save the registers of exit, which is
            // very convenient
            unsafe { core::arch::asm!("int 0xff") }
        } else {
            // no process to run, just wait for interrupts
            unsafe { cpu::halt() };
        }
    }
}

pub fn yield_current_if_any(all_state: &mut InterruptAllSavedState) {
    let current_cpu = cpu::cpu();
    if current_cpu.context.is_none() {
        return;
    }

    // save context of this process and mark is as scheduled
    {
        let mut processes = PROCESSES.lock();
        // TODO: find a better way to store processes or store process index/id.
        let process = processes
            .iter_mut()
            .find(|p| p.id == current_cpu.process_id)
            .expect("current process not found");
        assert!(process.state == ProcessState::Running);
        current_cpu.push_cli();
        swap_context(current_cpu.context.as_mut().unwrap(), all_state);
        // clear context from the CPU
        process.context = current_cpu.context.take().unwrap();
        process.state = ProcessState::Scheduled;
    }
    virtual_memory::switch_to_kernel();
    current_cpu.pop_cli();
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
    mem::swap(&mut all_state.rest.dr4, &mut context.dr4);
    mem::swap(&mut all_state.rest.dr5, &mut context.dr5);
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
    assert!(all_state.frame.cs & 0x3 == 0, "from user mode");
    let current_cpu = cpu::cpu();
    if current_cpu.context.is_none() {
        panic!("no context");
    }

    // this doesn't return, it does iretq directly
    swap_context(current_cpu.context.as_mut().unwrap(), all_state);
}
