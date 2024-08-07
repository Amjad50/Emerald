use core::{
    any::Any,
    ffi::c_void,
    panic::PanicInfo,
    sync::atomic::{AtomicI32, Ordering},
};

use alloc::boxed::Box;
use alloc::{string::String, vec::Vec};
use framehop::{
    x86_64::{CacheX86_64, UnwindRegsX86_64, UnwinderX86_64},
    ExplicitModuleSectionInfo, Module, Unwinder,
};
use unwinding::abi::{UnwindContext, UnwindReasonCode, _Unwind_Backtrace, _Unwind_GetIP};

use kernel_user_link::process::process_metadata;

use crate::{
    cpu::{self, idt::InterruptStackFrame64},
    hw::qemu,
    memory_management::memory_layout::{
        eh_frame_end, eh_frame_start, kernel_elf_end, kernel_text_end, KERNEL_LINK,
    },
    process::scheduler::with_current_process,
};

// this should be 'core-local/thread-local', but that's okay, as we want to halt the whole kernel
static PANIC_COUNT: AtomicI32 = AtomicI32::new(0);

pub fn print_kernel_stack_trace(rip: u64, rsp: u64, rbp: u64) {
    cpu::cpu().push_cli();

    let mut cache = CacheX86_64::<_>::new();
    let mut unwinder = UnwinderX86_64::new();

    let module = Module::new(
        String::from("kernel"),
        KERNEL_LINK as u64..kernel_elf_end() as u64,
        KERNEL_LINK as u64,
        ExplicitModuleSectionInfo {
            base_svma: KERNEL_LINK as _,
            text_svma: Some(KERNEL_LINK as _..kernel_text_end() as _),
            text: Some(unsafe {
                core::slice::from_raw_parts(KERNEL_LINK as _, kernel_text_end() - KERNEL_LINK)
            }),
            eh_frame_svma: Some(eh_frame_start() as _..eh_frame_end() as _),
            eh_frame: Some(unsafe {
                core::slice::from_raw_parts(
                    eh_frame_start() as _,
                    eh_frame_end() - eh_frame_start(),
                )
            }),
            ..Default::default()
        },
    );
    unwinder.add_module(module);

    let mut read_stack = |addr| Ok(unsafe { (addr as *const u64).read_volatile() });

    let mut iter = unwinder.iter_frames(
        rip,
        UnwindRegsX86_64::new(rip, rsp, rbp),
        &mut cache,
        &mut read_stack,
    );

    println!("Stack trace:");
    let mut i = 0;
    let mut frames = Vec::new();
    while let Ok(Some(frame)) = iter.next() {
        println!("{i:4}:{:#19x}", frame.address());
        frames.push(frame.address());
        i += 1;
    }

    print!("You can use this command to get information about the trace (since we don't have debug symbols here):\n$ addr2line -f -C -e ");
    #[cfg(debug_assertions)]
    print!("./target/x86-64-os/debug/kernel");
    #[cfg(not(debug_assertions))]
    print!("./target/x86-64-os/release/kernel");
    for frame in frames.iter() {
        print!(" {:#x}", frame);
    }
    println!();

    cpu::cpu().pop_cli();
}

pub fn print_process_stack_trace(frame: &InterruptStackFrame64, rbp: u64) {
    cpu::cpu().push_cli();

    assert_eq!(frame.cs & 0x3, 3, "We are in user mode");

    let meta = process_metadata();

    let module = Module::new(
        String::from("exe"),
        meta.image_base as _..(meta.image_base + meta.image_size) as _,
        meta.image_base as _,
        ExplicitModuleSectionInfo {
            base_svma: meta.image_base as _,
            text_svma: Some(meta.text_address as _..(meta.text_address + meta.text_size) as _),
            text: Some(unsafe {
                core::slice::from_raw_parts(meta.text_address as _, meta.text_size)
            }),
            eh_frame_svma: Some(
                meta.eh_frame_address as _..(meta.eh_frame_address + meta.eh_frame_size) as _,
            ),
            eh_frame: Some(unsafe {
                core::slice::from_raw_parts(meta.eh_frame_address as _, meta.eh_frame_size)
            }),
            ..Default::default()
        },
    );
    let mut cache = CacheX86_64::new();
    let mut unwinder: UnwinderX86_64<&[u8]> = UnwinderX86_64::new();
    unwinder.add_module(module);

    let mut read_stack = |addr| Ok(unsafe { (addr as *const u64).read_volatile() });

    let mut iter = unwinder.iter_frames(
        frame.rip as _,
        UnwindRegsX86_64::new(frame.rip as _, frame.rsp as _, rbp as _),
        &mut cache,
        &mut read_stack,
    );

    println!("Stack trace:");
    let mut i = 0;
    let mut frames = Vec::new();
    while let Ok(Some(frame)) = iter.next() {
        println!("{i:4}:{:#19x}", frame.address());
        frames.push(frame.address());
        i += 1;
    }

    with_current_process(|process| {
        print!("You can use this command to get information about the trace (since we don't have debug symbols here):\n$ addr2line -f -C -e ./filesystem{}", process.file_path().as_str());
    });
    for frame in frames.iter() {
        print!(" {:#x}", frame);
    }
    println!();

    cpu::cpu().pop_cli();
}

pub fn print_originating_stack_trace(frame: &InterruptStackFrame64, rbp: u64) {
    if frame.cs & 0x3 == 3 {
        print_process_stack_trace(frame, rbp);
    } else {
        print_kernel_stack_trace(frame.rip, frame.rsp, rbp);
    }
}

#[allow(dead_code)]
pub fn catch_unwind<R, F: FnOnce() -> R>(f: F) -> Result<R, Box<dyn Any + Send>> {
    unwinding::panic::catch_unwind(f).map_err(|e| {
        // reset panic count
        PANIC_COUNT.store(0, Ordering::Relaxed);
        e
    })
}

fn stack_trace() {
    cpu::cpu().push_cli();
    struct CallbackData {
        counter: usize,
    }
    extern "C" fn callback(unwind_ctx: &UnwindContext<'_>, arg: *mut c_void) -> UnwindReasonCode {
        let data = unsafe { &mut *(arg as *mut CallbackData) };
        data.counter += 1;
        println!("{:4}:{:#19x}", data.counter, _Unwind_GetIP(unwind_ctx));
        UnwindReasonCode::NO_REASON
    }
    let mut data = CallbackData { counter: 0 };
    _Unwind_Backtrace(callback, &mut data as *mut _ as _);

    print!("You can use this command to get information about the trace (since we don't have debug symbols here):\n$ addr2line -f -C -e ");
    #[cfg(debug_assertions)]
    print!("./target/x86-64-os/debug/kernel");
    #[cfg(not(debug_assertions))]
    print!("./target/x86-64-os/release/kernel");
    extern "C" fn callback2(unwind_ctx: &UnwindContext<'_>, _arg: *mut c_void) -> UnwindReasonCode {
        print!(" {:#x}", _Unwind_GetIP(unwind_ctx));
        UnwindReasonCode::NO_REASON
    }
    _Unwind_Backtrace(callback2, core::ptr::null_mut() as _);
    println!("\nhalting...");

    cpu::cpu().pop_cli();
}

fn panic_trace(msg: Box<dyn Any + Send>) -> ! {
    if PANIC_COUNT.load(Ordering::Relaxed) >= 1 {
        stack_trace();
        println!("thread panicked while processing panic. halting...");

        qemu::exit(qemu::ExitStatus::Failure);
    }
    PANIC_COUNT.store(1, Ordering::Relaxed);
    stack_trace();

    let code = unwinding::panic::begin_panic(Box::new(msg));
    println!(
        "failed to initiate panic, maybe, no one is catching it?. got code: {} halting...",
        code.0
    );

    qemu::exit(qemu::ExitStatus::Failure);
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    unsafe { cpu::clear_interrupts() };
    println!("{}", info);

    struct NoPayload;
    panic_trace(Box::new(NoPayload))
}
