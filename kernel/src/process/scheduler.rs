use core::{hint, mem};

use crate::{
    cpu::{self, idt::InterruptStackFrame64, interrupts},
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
                current_cpu.context = None;
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
        }
    }
}

/// # Safety
///  must ensure the context is valid, as it will be used to restore the context
pub unsafe fn get_current_context(frame: &InterruptStackFrame64, context: &mut ProcessContext) {
    core::arch::x86_64::_fxsave64(context.fxsave.0.as_mut_ptr() as _);
    // save in reverse order using the stack
    core::arch::asm!(
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rsp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "mov rax, dr7
         push rax",
        "mov rax, dr6
         push rax",
        "mov rax, dr5
         push rax",
        "mov rax, dr4
         push rax",
        "mov rax, dr3
         push rax",
        "mov rax, dr2
         push rax",
        "mov rax, dr1
         push rax",
        "mov rax, dr0
         push rax",
        "mov rax, ss
         push rax",
        "mov rax, gs
         push rax",
        "mov rax, fs
         push rax",
        "mov rax, es
         push rax",
        "mov rax, ds
         push rax",
        "mov rax, cs
         push rax",
        "lea rax, [rip + 1f]
         1:
         push rax",
        "pushfq",
        "mov rcx, r14",
        "mov rsi, rsp",
        "mov rdi, r15",
        // copy content of rsp to current_context
        "rep movsb",
        "mov rsp, rsi",
        in("r14") mem::size_of::<ProcessContext>() - mem::size_of::<FxSave>(),
        in("r15") context as *mut _ as *mut u8,
        // tell the compiler that all registers will be changed, so don't play with them
        out("rax") _,
        out("rcx") _,
        out("rdx") _,
        out("rsi") _,
        out("rdi") _,
        out("r8") _,
        out("r9") _,
        out("r10") _,
        out("r11") _,
        out("r12") _,
        out("r13") _,
        options(readonly, nostack)
    );
    context.rip = frame.rip;
    context.rflags = frame.rflags;
    context.cs = frame.cs as _;
    context.rsp = frame.rsp;
    context.ss = frame.ss as _;
}

/// # Safety
///  must ensure the context is valid, as it will be used to restore the context
pub unsafe fn set_context_and_iretq(context: &ProcessContext, frame: &InterruptStackFrame64) -> ! {
    core::arch::x86_64::_fxrstor64(context.fxsave.0.as_ptr() as _);
    // restore in reverse order using the stack
    core::arch::asm!(
        "mov rsp, r15",
        "pop rax",  // popfq: don't care, we will put it in the frame
        "pop rax",  // pop rip: don't care, we will put it in the frame
        "pop rax",  // pop cs: don't care, we will put it in the frame
        "pop rax
         mov ds, ax",
        "pop rax
         mov es, ax",
        "pop rax
         mov fs, ax",
        "pop rax
         mov gs, ax",
        "pop rax",  // pop ss: don't care, we will put it in the frame
        "pop rax
         mov dr0, rax",
        "pop rax
         mov dr1, rax",
        "pop rax
         mov dr2, rax",
        "pop rax
         mov dr3, rax",
        "pop rax
         mov dr4, rax",
        "pop rax
         mov dr5, rax",
        "pop rax
         mov dr6, rax",
        "pop rax
         mov dr7, rax",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rax",  // pop rsp: don't care, we will put it in the frame
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "mov rax, r14",
        "pop r14",
        "pop r15",
        "mov rsp, rax",
        "iretq",
        in("r15") context as *const _ as *const u8,
        in("r14") frame as *const _ as *const u8,
        options(nostack, noreturn),


    );
}

extern "x86-interrupt" fn scheduler_interrupt_handler(mut frame: InterruptStackFrame64) {
    let mut current_context = Default::default();
    unsafe { get_current_context(&frame, &mut current_context) };
    assert!(frame.cs & 0x3 == 0, "from user mode");
    println!("scheduling...");

    let current_cpu = cpu::cpu();
    if current_cpu.context.is_none() {
        panic!("no context");
    }
    let new_context = current_cpu.context.take().unwrap();
    current_cpu.context = Some(current_context);
    // copy to the frame, so we can return immediately
    frame.rip = new_context.rip;
    frame.rflags = new_context.rflags;
    frame.cs = new_context.cs as _;
    frame.rsp = new_context.rsp;
    frame.ss = new_context.ss as _;
    // this doesn't return, it does iretq directly
    unsafe { set_context_and_iretq(&new_context, &frame) };
}
