#![no_std]
#![no_main]

use crate::{memory_layout::kernel_size, multiboot::MultiBootInfoRaw, video_memory::MemSize};

pub mod memory_layout;
mod multiboot;
mod video_memory;

core::arch::global_asm!(include_str!("boot.S"));

macro_rules! pause {
    () => {
        unsafe {
            core::arch::asm!("pause");
        }
    };
}

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info_ptr: u64) -> ! {
    let mut vga_buffer = video_memory::VgaBuffer::new();

    println!(vga_buffer, "Multiboot is at: {:x}", multiboot_info_ptr);
    let multiboot_info = unsafe { MultiBootInfoRaw::from_ptr(multiboot_info_ptr) };
    println!(vga_buffer, "{multiboot_info}");

    let high_mem_size = multiboot_info.upper_memory_size().unwrap();
    let kernel_size = kernel_size();
    let remaining_space = high_mem_size - kernel_size;

    println!(
        vga_buffer,
        "Remaining size: {}, kernel size {}",
        MemSize(remaining_space),
        MemSize(kernel_size),
    );

    loop {
        pause!();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut vga_buffer = video_memory::VgaBuffer::new();
    println!(vga_buffer, "{info}");
    loop {
        pause!();
    }
}
