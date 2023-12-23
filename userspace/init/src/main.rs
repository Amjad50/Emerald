#![no_std]
#![no_main]

use core::ffi::{c_char, CStr};

use common::{
    call_syscall,
    syscalls::{SYS_EXIT, SYS_INC_HEAP, SYS_SPAWN, SYS_WRITE},
    FD_STDOUT,
};

fn write_to_stdout(s: &CStr) {
    unsafe {
        call_syscall!(
            SYS_WRITE,
            FD_STDOUT,                 // fd
            s.as_ptr() as u64,         // buf
            s.to_bytes().len() as u64  // size
        )
        .unwrap();
    }
}

fn exit(code: u64) -> ! {
    unsafe {
        call_syscall!(
            SYS_EXIT,
            code, // code
        )
        .unwrap();
    }
    unreachable!("exit syscall should not return")
}

fn spawn(path: &CStr, argv: &[*const c_char]) -> u64 {
    unsafe {
        call_syscall!(
            SYS_SPAWN,
            path.as_ptr() as u64, // path
            argv.as_ptr() as u64, // argv
        )
        .unwrap()
    }
}

fn allocate_heap(size: usize) -> *mut u8 {
    unsafe {
        call_syscall!(
            SYS_INC_HEAP,
            size as u64, // increment
        )
        .unwrap() as *mut u8
    }
}

fn get_heap_end_ptr() -> *mut u8 {
    unsafe {
        call_syscall!(
            SYS_INC_HEAP,
            0, // increment
        )
        .unwrap() as *mut u8
    }
}

fn convert_to_string(mut number: i64, radix: u8, buffer: &mut [u8]) -> usize {
    assert!((2..=16).contains(&radix));
    const RADIX_CHARS: &[u8] = b"0123456789ABCDEF";

    let mut i = 0;
    if number < 0 {
        buffer[i] = b'-';
        i += 1;
        number = -number;
    }
    let mut n = number as u64;
    if n == 0 {
        buffer[i] = b'0';
        i += 1;
    } else {
        let mut len = 0;
        while n > 0 {
            len += 1;
            n /= radix as u64;
        }
        let mut n = number as u64;
        while n > 0 {
            i += 1;
            buffer[len - i] = RADIX_CHARS[(n % radix as u64) as usize];
            n /= radix as u64;
        }
    }
    i
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    // create some delay
    write_to_stdout(c"[init] Hello!\n\n");

    let shell_path = c"/shell";
    let shell_argv = [shell_path.as_ptr(), c"".as_ptr()];
    let shell_pid = spawn(shell_path, &shell_argv);

    let mut buf = [0u8; 100];
    let msg = c"[init] spawned shell with pid ";
    let msg_len = msg.to_bytes().len();
    buf[..msg_len].copy_from_slice(msg.to_bytes());
    let len = convert_to_string(shell_pid as _, 10, &mut buf[msg_len..]);
    buf[msg_len + len] = b'\n';
    write_to_stdout(unsafe { CStr::from_bytes_with_nul_unchecked(&buf) });

    let heap_size = 0x1000;
    let heap_start = allocate_heap(heap_size);
    assert!(get_heap_end_ptr() == unsafe { heap_start.add(heap_size) });

    // use the data in the heap to print the heap start ptr
    let arr_at_heap = unsafe { core::slice::from_raw_parts_mut(heap_start, heap_size) };
    // zero out
    arr_at_heap.fill(0);
    let msg = c"[init] Got new heap at: 0x";
    arr_at_heap[..msg.to_bytes().len()].copy_from_slice(msg.to_bytes());
    // put the heap start ptr as integer
    let len = convert_to_string(
        heap_start as _,
        16,
        &mut arr_at_heap[msg.to_bytes().len()..],
    );
    arr_at_heap[msg.to_bytes().len() + len] = b'\n';
    // print
    write_to_stdout(unsafe { CStr::from_bytes_with_nul_unchecked(arr_at_heap) });
    exit(111);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let mut buf = [0u8; 100];
    let msg = c"[init] panicked!, line: ";
    buf[..msg.to_bytes().len()].copy_from_slice(msg.to_bytes());
    let line = _info.location().unwrap().line();
    let len = convert_to_string(line as _, 10, &mut buf[msg.to_bytes().len()..]);
    buf[msg.to_bytes().len() + len] = b'\n';
    write_to_stdout(unsafe { CStr::from_bytes_with_nul_unchecked(&buf) });
    // write_to_stdout(c"[init] panicked!\n");
    exit(0xFF);
}
