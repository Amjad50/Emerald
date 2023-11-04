#![no_std]
#![no_main]

extern crate alloc;

// boot assembly code
// starts in protected mode, setup long mode and jumps to kernel_main
core::arch::global_asm!(include_str!("boot.S"));

#[macro_use]
// import first so that macros are available in other modules
mod macros;

mod cpu;
mod io;
mod memory_management;
mod multiboot;
mod sync;

use core::hint;

use crate::{
    io::console,
    memory_management::{
        memory_layout::{kernel_end, kernel_size, PAGE_4K},
        physical_page_allocator::{self, PAGE_ALLOCATOR_SIZE},
        virtual_memory,
    },
    multiboot::MultiBootInfoRaw,
};

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info_ptr: usize) -> ! {
    // initialize the console (printing)
    console::init();

    println!("Multiboot is at: {:x}", multiboot_info_ptr);
    let multiboot_info = unsafe { MultiBootInfoRaw::from_ptr(multiboot_info_ptr) };
    println!("{multiboot_info}");

    let high_mem_size = multiboot_info.upper_memory_size().unwrap();
    let kernel_size = kernel_size();
    let remaining_space = high_mem_size - kernel_size;

    let pages_to_use = (remaining_space).min(PAGE_ALLOCATOR_SIZE) / PAGE_4K;
    // initialize the physical page allocator
    // must be called before any pages can be allocated
    physical_page_allocator::init(kernel_end() as *mut u8, pages_to_use);
    virtual_memory::init_kernel_vm();

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
