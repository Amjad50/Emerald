#![no_std]
#![no_main]

use crate::{
    memory_layout::{kernel_end, kernel_size, PAGE_4K},
    multiboot::MultiBootInfoRaw,
    physical_page_allocator::PAGE_ALLOCATOR_SIZE,
};

mod cpu;
pub mod memory_layout;
mod multiboot;
mod physical_page_allocator;
mod sync;
mod video_memory;

core::arch::global_asm!(include_str!("boot.S"));

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info_ptr: usize) -> ! {
    let mut vga_buffer = video_memory::VgaBuffer::new();

    println!(vga_buffer, "Multiboot is at: {:x}", multiboot_info_ptr);
    let multiboot_info = unsafe { MultiBootInfoRaw::from_ptr(multiboot_info_ptr) };
    println!(vga_buffer, "{multiboot_info}");

    let high_mem_size = multiboot_info.upper_memory_size().unwrap();
    let kernel_size = kernel_size();
    let remaining_space = high_mem_size - kernel_size;

    let pages_to_use = (remaining_space).min(PAGE_ALLOCATOR_SIZE) / PAGE_4K;
    physical_page_allocator::init(kernel_end() as *mut u8, pages_to_use);

    loop {
        cpu::pause!();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut vga_buffer = video_memory::VgaBuffer::new();
    println!(vga_buffer, "{info}");
    loop {
        cpu::pause!();
    }
}
