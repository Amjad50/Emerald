#![no_std]
#![no_main]

// The virtual address of the kernel
pub static KERNEL_BASE: u64 = 0xFFFFFFFF80000000;
pub static TEXT_OFFSET: u64 = 0x100000;
pub static KERNEL_LINK: u64 = KERNEL_BASE + TEXT_OFFSET;

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
pub extern "C" fn kernel_main(_multiboot_info_ptr: u64) -> ! {
    loop {
        pause!();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
