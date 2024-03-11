use core::{
    ffi::c_void,
    hint,
    panic::PanicInfo,
    sync::atomic::{AtomicI32, Ordering},
};

use unwinding::abi::{UnwindContext, UnwindReasonCode, _Unwind_Backtrace, _Unwind_GetIP};

use crate::cpu;

// this should be 'core-local/thread-local', but that's okay, as we want to halt the whole kernel
static PANIC_COUNT: AtomicI32 = AtomicI32::new(0);

fn stack_trace() {
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

    print!("You can use this command to get information about the trace (since we don't have debug symbols here):\n$ addr2line -f -C -e ./target/x86-64-os/debug/kernel");
    extern "C" fn callback2(unwind_ctx: &UnwindContext<'_>, _arg: *mut c_void) -> UnwindReasonCode {
        print!(" {:#x}", _Unwind_GetIP(unwind_ctx));
        UnwindReasonCode::NO_REASON
    }
    _Unwind_Backtrace(callback2, core::ptr::null_mut() as _);
    println!("\nhalting...");
}

fn panic_trace() -> ! {
    if PANIC_COUNT.load(Ordering::Relaxed) >= 1 {
        stack_trace();
        println!("thread panicked while processing panic. halting...");
        abort();
    }
    PANIC_COUNT.store(1, Ordering::Relaxed);
    stack_trace();
    println!("failed to initiate panic, maybe, we don't have `eh_frame` data?. halting...");
    abort()
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    unsafe { cpu::clear_interrupts() };
    println!("{}", info);

    panic_trace()
}

fn abort() -> ! {
    loop {
        unsafe {
            cpu::halt();
        }
        hint::spin_loop();
    }
}
