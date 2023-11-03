#![no_std]
#![no_main]

// boot assembly code
// starts in protected mode, setup long mode and jumps to kernel_main
core::arch::global_asm!(include_str!("boot.S"));

#[macro_use]
// import first so that macros are available in other modules
mod macros;

mod cpu;
mod io;
pub mod memory_layout;
mod multiboot;
mod physical_page_allocator;
mod sync;

use core::hint;

use crate::{
    io::console,
    memory_layout::{kernel_end, kernel_size, PAGE_4K},
    multiboot::MultiBootInfoRaw,
    physical_page_allocator::PAGE_ALLOCATOR_SIZE,
};

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info_ptr: usize) -> ! {
    console::init();

    println!("Multiboot is at: {:x}", multiboot_info_ptr);
    let multiboot_info = unsafe { MultiBootInfoRaw::from_ptr(multiboot_info_ptr) };
    println!("{multiboot_info}");

    let high_mem_size = multiboot_info.upper_memory_size().unwrap();
    let kernel_size = kernel_size();
    let remaining_space = high_mem_size - kernel_size;

    let pages_to_use = (remaining_space).min(PAGE_ALLOCATOR_SIZE) / PAGE_4K;
    physical_page_allocator::init(kernel_end() as *mut u8, pages_to_use);

    loop {
        hint::spin_loop();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    unsafe { cpu::clear_interrupts() };
    println!("{info}");
    loop {
        hint::spin_loop();
    }
}
