#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(linked_list_cursors)]

extern crate alloc;

// boot assembly code
// starts in protected mode, setup long mode and jumps to kernel_main
core::arch::global_asm!(include_str!("boot.S"));

#[macro_use]
// import first so that macros are available in other modules
mod macros;

mod acpi;
mod collections;
mod cpu;
mod devices;
mod executable;
mod fs;
mod io;
mod memory_management;
mod multiboot2;
pub mod process;
mod sync;

use core::hint;

use common::{FD_STDERR, FD_STDIN, FD_STDOUT};
use cpu::{
    gdt,
    interrupts::{self, apic},
};
use executable::elf::Elf;
use io::console;
use memory_management::virtual_memory_mapper;
use multiboot2::MultiBoot2Info;
use process::scheduler;

use crate::{
    devices::clock,
    memory_management::{
        kernel_heap_allocator::ALLOCATOR,
        memory_layout::{self, MemSize, KERNEL_HEAP_SIZE, PAGE_4K},
        physical_page_allocator, virtual_space,
    },
    process::Process,
};

fn finish_boot() {
    let physical_pages_stats = physical_page_allocator::stats();
    let free_mem = MemSize((physical_pages_stats.0 * PAGE_4K) as u64);
    let used_mem = MemSize((physical_pages_stats.1 * PAGE_4K) as u64);
    // this stats is recorded at this point, meaning that we could have allocated a lot,
    //  but then it got freed we don't record that
    let (free_heap, used_heap) = ALLOCATOR.stats();
    println!("\n\nBoot finished!");
    memory_layout::display_kernel_map();
    println!("Free memory: {}", free_mem);
    println!(
        "Used memory: {} ({:0.3}%)",
        used_mem,
        used_mem.0 as f64 / (used_mem.0 + free_mem.0) as f64 * 100.
    );
    println!("Free heap: {}", MemSize(free_heap as u64));
    println!(
        "Used heap: {} ({:0.3}%)",
        MemSize(used_heap as u64),
        used_heap as f64 / (used_heap + free_heap) as f64 * 100.
    );
    println!(
        "From possible heap: {} ({:0.3}%)",
        MemSize(KERNEL_HEAP_SIZE as u64),
        used_heap as f64 / KERNEL_HEAP_SIZE as f64 * 100.
    );
    virtual_space::debug_blocks();
    println!();
}

fn load_init_process() {
    let mut init_file = fs::open("/init").expect("Could not find `init` file");
    let elf = Elf::load(&mut init_file).expect("Could not load init file");
    let mut process = Process::allocate_process(0, &elf, &mut init_file)
        .expect("Could not allocate process for `init`");
    assert!(process.id() == 0, "Must be the first process");

    // add the console to `init` manually, after that processes will either inherit it or open a pipe or something
    // to act as STDIN/STDOUT/STDERR
    let console = fs::open("/devices/console").expect("Could not find `/devices/console`");
    process.attach_file_to_fd(FD_STDIN, console.clone());
    process.attach_file_to_fd(FD_STDOUT, console.clone());
    process.attach_file_to_fd(FD_STDERR, console);

    println!("Added `init` process pid={}", process.id());
    scheduler::push_process(process);
}

#[link_section = ".text"]
#[no_mangle]
pub extern "C" fn kernel_main(multiboot_info: &MultiBoot2Info) -> ! {
    // init console first, so if we panicked, we can still see the output
    console::early_init();
    println!("{}", multiboot_info);
    // must be called before any pages can be allocated
    physical_page_allocator::init(multiboot_info);
    // must be called next, before GDT, and this must be called before any heap allocations
    virtual_memory_mapper::init_kernel_vm();
    // must be called before interrupts
    gdt::init_kernel_gdt();
    interrupts::init_interrupts();
    // mount
    devices::init_devices_mapping();
    let bios_tables = acpi::get_acpi_tables(multiboot_info).expect("BIOS tables not found");
    println!("BIOS tables: {}", bios_tables);
    apic::init(&bios_tables);
    clock::init(&bios_tables);
    console::init_late_device();
    unsafe { cpu::set_interrupts() };
    devices::prope_pci_devices();
    fs::create_disk_mapping(0).expect("Could not load filesystem");
    finish_boot();
    // -- BOOT FINISHED --

    load_init_process();

    // this will never return
    scheduler::schedule()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    unsafe { cpu::clear_interrupts() };
    println!("{info}");
    loop {
        unsafe {
            cpu::halt();
        }
        hint::spin_loop();
    }
}
