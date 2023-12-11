#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(const_mut_refs)]

extern crate alloc;

// boot assembly code
// starts in protected mode, setup long mode and jumps to kernel_main
core::arch::global_asm!(include_str!("boot.S"));

#[macro_use]
// import first so that macros are available in other modules
mod macros;

mod bios;
mod collections;
mod cpu;
mod devices;
mod executable;
mod fs;
mod io;
mod memory_management;
mod multiboot;
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
use memory_management::{
    memory_layout::{
        kernel_elf_end, EXTENDED_BIOS_BASE_PHYSICAL, EXTENDED_BIOS_SIZE, EXTENDED_OFFSET,
        KERNEL_END, KERNEL_MAPPED_SIZE, ONE_MB,
    },
    virtual_memory,
};
use multiboot::{MemoryMapType, MultiBootInfoRaw};
use process::scheduler;

use crate::{
    devices::clock,
    memory_management::{
        kernel_heap_allocator::ALLOCATOR,
        memory_layout::{MemSize, KERNEL_HEAP_SIZE, PAGE_4K},
        physical_page_allocator,
    },
    process::Process,
};

/// Checks that we have enough memory, and keep note of where the kernel ends
/// and where the extended BIOS data starts after the kernel (not static)
/// so that we can make later on
fn check_and_setup_memory(multiboot_info: &MultiBootInfoRaw) {
    // Upper memory + 1MB since it starts from 1MB offset
    let mem_size = multiboot_info.upper_memory_size().unwrap() + ONE_MB as u64;
    // check that we have enough space to map all the data we want in the kernel
    if mem_size < KERNEL_MAPPED_SIZE as u64 {
        // If you specify `-m 128` in qemu, this will crash, since qemu doesn't exactly give 128MB, I think some
        // of this memory is reserved and used by the BIOS, so you will get `127` or `126` MB instead.
        panic!(
            "Not enough memory, need at least {}, got {}",
            MemSize(KERNEL_MAPPED_SIZE as u64),
            MemSize(mem_size)
        );
    }
    let mmap = multiboot_info.memory_maps().unwrap();
    let mut got_middle_memory = false;
    for entry in mmap {
        match entry.mem_type {
            MemoryMapType::Available => {
                if entry.base_addr == EXTENDED_OFFSET as u64 {
                    got_middle_memory = true;
                }
            }
            MemoryMapType::Reserved if got_middle_memory => {
                unsafe {
                    EXTENDED_BIOS_BASE_PHYSICAL = entry.base_addr as usize;
                    EXTENDED_BIOS_SIZE = entry.length as usize;
                }
                break;
            }
            _ => {}
        }
    }
}

fn finish_boot() {
    let physical_pages_stats = physical_page_allocator::stats();
    let free_mem = MemSize((physical_pages_stats.0 * PAGE_4K) as u64);
    let used_mem = MemSize((physical_pages_stats.1 * PAGE_4K) as u64);
    // this stats is recorded at this point, meaning that we could have allocated a lot,
    //  but then it got freed we don't record that
    let (free_heap, used_heap) = ALLOCATOR.stats();
    println!("\n\nBoot finished!");
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
pub extern "C" fn kernel_main(multiboot_info: &MultiBootInfoRaw) -> ! {
    // init console first, so if we panicked, we can still see the output
    console::early_init();
    println!("{}", multiboot_info);
    check_and_setup_memory(multiboot_info);
    // must be called before any pages can be allocated
    physical_page_allocator::init(kernel_elf_end() as _, KERNEL_END as _);
    // must be called next, before GDT, and this must be called before any heap allocations
    virtual_memory::init_kernel_vm();
    // must be called before interrupts
    gdt::init_kernel_gdt();
    interrupts::init_interrupts();
    // mount
    devices::init_devices_mapping();
    // TODO: handle for UEFI
    let bios_tables = bios::tables::get_bios_tables().expect("BIOS tables not found");
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
        hint::spin_loop();
    }
}
