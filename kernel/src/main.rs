#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(linked_list_cursors)]
#![feature(const_binary_heap_constructor)]
#![feature(btree_extract_if)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]
// fix warnings when testing (since we are not using the normal `kernel_main`)
#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(test, allow(unused_imports))]

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
mod graphics;
mod hw;
mod io;
mod memory_management;
mod multiboot2;
mod panic_handler;
mod process;
mod sync;
mod testing;

use alloc::vec::Vec;
use cpu::{
    gdt,
    interrupts::{self, apic},
};
use executable::elf::Elf;
use increasing_heap_allocator::HeapStats;
use io::console;
use kernel_user_link::{
    file::{BlockingMode, OpenOptions},
    FD_STDERR, FD_STDIN, FD_STDOUT,
};
use memory_management::virtual_memory_mapper;
use multiboot2::MultiBoot2Info;
use process::scheduler;
use tracing::info;

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
    let free_mem = MemSize(physical_pages_stats.0 * PAGE_4K);
    let used_mem = MemSize(physical_pages_stats.1 * PAGE_4K);
    // this stats is recorded at this point, meaning that we could have allocated a lot,
    //  but then it got freed we don't record that
    let HeapStats {
        allocated,
        free_size,
        heap_size,
    } = ALLOCATOR.stats();
    info!("Boot finished!");
    memory_layout::display_kernel_map();
    info!("Free memory: {}", free_mem);
    info!(
        "Used memory: {} ({:0.3}%)",
        used_mem,
        used_mem.0 as f64 / (used_mem.0 + free_mem.0) as f64 * 100.
    );
    info!("Free heap: {}", MemSize(free_size));
    info!(
        "Used heap: {} ({:0.3}%)",
        MemSize(allocated),
        allocated as f64 / (heap_size) as f64 * 100.
    );
    info!(
        "From possible heap: {} ({:0.3}%)",
        MemSize(KERNEL_HEAP_SIZE),
        allocated as f64 / KERNEL_HEAP_SIZE as f64 * 100.
    );
    virtual_space::debug_blocks();
    info!("");
}

fn load_init_process() {
    let mut init_file = fs::File::open("/init").expect("Could not find `init` file");
    let elf = Elf::load(&mut init_file).expect("Could not load init file");
    let mut process = Process::allocate_process(
        0,
        &elf,
        &mut init_file,
        Vec::new(),
        fs::Directory::open("/").expect("No root"),
    )
    .expect("Could not allocate process for `init`");
    assert!(process.id() == 0, "Must be the first process");

    // add the console to `init` manually, after that processes will either inherit it or open a pipe or something
    // to act as STDIN/STDOUT/STDERR
    let mut console = fs::File::open_blocking(
        "/devices/console",
        BlockingMode::Line,
        OpenOptions::READ | OpenOptions::WRITE,
    )
    .expect("Could not find `/devices/console`");
    // mark it as `terminal`
    console.set_terminal(true);
    process.attach_fs_node_to_fd(FD_STDIN, console.clone_inherit());
    process.attach_fs_node_to_fd(FD_STDOUT, console.clone_inherit());
    process.attach_fs_node_to_fd(FD_STDERR, console);

    info!("Added `init` process pid={}", process.id());
    scheduler::push_process(process);
}

#[link_section = ".text"]
#[no_mangle]
#[cfg(not(test))]
pub extern "C" fn kernel_main(multiboot_info: &MultiBoot2Info) -> ! {
    // init console first, so if we panicked, we can still see the output
    console::early_init();
    console::tracing::init();
    info!("{}", multiboot_info);
    // must be called before any pages can be allocated
    physical_page_allocator::init(multiboot_info);
    // must be called next, before GDT, and this must be called before any heap allocations
    virtual_memory_mapper::init_kernel_vm();
    // require heap allocation
    console::tracing::move_to_dynamic_buffer();
    // must be called before interrupts
    gdt::init_kernel_gdt();
    interrupts::init_interrupts();
    // mount devices map before initializing them
    devices::init_devices_mapping();
    let bios_tables = acpi::get_acpi_tables(multiboot_info).expect("BIOS tables not found");
    info!("BIOS tables: {}", bios_tables);
    apic::init(bios_tables);
    clock::init(bios_tables);
    // APIC timer interrupt rely on the clock, so it must be initialized after the clock
    // and interrupts should be disabled until
    unsafe { cpu::set_interrupts() };
    devices::init_legacy_devices();
    graphics::vga::init(multiboot_info.framebuffer());
    console::init_late_device(multiboot_info.framebuffer());
    devices::prope_pci_devices();
    fs::create_disk_mapping(0).expect("Could not load filesystem");
    finish_boot();
    // -- BOOT FINISHED --

    load_init_process();

    // this will never return
    scheduler::schedule()
}

#[link_section = ".text"]
#[no_mangle]
#[cfg(test)]
pub extern "C" fn kernel_main(multiboot_info: &MultiBoot2Info) -> ! {
    // perform necessary initialization, then call the test
    console::early_init();
    physical_page_allocator::init(multiboot_info);
    virtual_memory_mapper::init_kernel_vm();

    test_main();

    loop {
        unsafe {
            cpu::halt();
        }
    }
}
