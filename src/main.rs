#![no_std]
#![no_main]

use crate::multiboot::MultiBootInfoRaw;

mod multiboot;
mod video_memory;

core::arch::global_asm!(include_str!("boot.S"));

// The virtual address of the kernel
pub static KERNEL_BASE: u64 = 0xFFFFFFFF80000000;
pub static TEXT_OFFSET: u64 = 0x100000;
pub static KERNEL_LINK: u64 = KERNEL_BASE + TEXT_OFFSET;

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
