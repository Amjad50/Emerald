#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

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

use cpu::{gdt, interrupts};
use memory_management::memory_layout::{KERNEL_END, KERNEL_MAPPED_SIZE, ONE_MB};

use crate::{
    io::console,
    memory_management::{
        memory_layout::{kernel_end, MemSize, PAGE_4K},
        physical_page_allocator::{self},
        virtual_memory,
    },
    multiboot::MultiBootInfoRaw,
};

fn check_memory(multiboot_info: &MultiBootInfoRaw) {
    // Upper memory + 1MB since it starts from 1MB offset
    let mem_size = multiboot_info.upper_memory_size().unwrap() + ONE_MB;
    // check that we have enough space to map all the data we want in the kernel
    if mem_size < KERNEL_MAPPED_SIZE {
        // If you specify `-m 128` in qemu, this will crash, since qemu doesn't exactly give 128MB, I think some
        // of this memory is reserved and used by the BIOS, so you will get `127` or `126` MB instead.
        panic!(
            "Not enough memory, need at least {}, got {}",
            MemSize(KERNEL_MAPPED_SIZE),
            MemSize(mem_size)
        );
    }
}

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info: &MultiBootInfoRaw) -> ! {
    // init console first, so if we panicked, we can still see the output
    console::init();
    check_memory(multiboot_info);
    // must be called before any pages can be allocated
    physical_page_allocator::init(kernel_end() as _, KERNEL_END as _);
    // must be called next, before GDT, and this must be called before any heap allocations
    virtual_memory::init_kernel_vm();
    gdt::init_kernel_gdt();
    interrupts::init_interrupts();

    let physical_pages_stats = physical_page_allocator::stats();
    let free_mem = MemSize(physical_pages_stats.0 * PAGE_4K);
    let used_mem = MemSize(physical_pages_stats.1 * PAGE_4K);
    println!("Boot finished!");
    println!("Free memory: {}", free_mem);
    println!("Used memory: {}", used_mem);
    println!(
        "Used %: {:0.2}",
        used_mem.0 as f64 / (used_mem.0 + free_mem.0) as f64 * 100.0
    );
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
