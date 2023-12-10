pub mod apic;
mod handlers;

use crate::sync::spin::mutex::Mutex;

use super::{
    gdt::USER_RING,
    idt::{BasicInterruptHandler, InterruptDescriptorTable, InterruptHandlerWithAllState},
};

static INTERRUPTS: Mutex<Interrupts> = Mutex::new(Interrupts::empty());

pub(super) mod stack_index {
    pub const FAULTS_STACK: u8 = 0;
    pub const DOUBLE_FAULT_STACK: u8 = 1;
    pub const SYSCALL_STACK: u8 = 2;
}

const USER_INTERRUPTS_START: u8 = 0x20;
const MAX_USER_INTERRUPTS: u8 = 0xe0 - 0x10;
pub const SPECIAL_SCHEDULER_INTERRUPT: u8 = 0xdf; // last one (0xFF)
pub const SPECIAL_SYSCALL_INTERRUPT: u8 = 0xde; // second last one (0xFE)

struct Interrupts {
    idt: InterruptDescriptorTable,
    last_used_user_interrupt: u16,
}

impl Interrupts {
    const fn empty() -> Self {
        Self {
            idt: InterruptDescriptorTable::empty(),
            last_used_user_interrupt: 0,
        }
    }

    // only apply init for static context
    fn init(&'static mut self) {
        self.idt.init_default_handlers();

        // this is only done once
        self.idt.apply_idt();
    }

    fn get_next_interrupt(&mut self) -> u8 {
        if self.last_used_user_interrupt >= MAX_USER_INTERRUPTS as u16 {
            panic!("No more user interrupts available");
        }

        let interrupt = self.last_used_user_interrupt;
        self.last_used_user_interrupt += 1;

        interrupt as u8
    }

    fn allocate_basic_user_interrupt(&mut self, handler: BasicInterruptHandler) -> u8 {
        let interrupt = self.get_next_interrupt();

        self.idt.user_defined[interrupt as usize].set_handler(handler);

        interrupt + USER_INTERRUPTS_START
    }

    fn allocate_user_interrupt_all_saved(&mut self, handler: InterruptHandlerWithAllState) -> u8 {
        let interrupt = self.get_next_interrupt();

        self.idt.user_defined[interrupt as usize]
            .set_handler_with_number(handler, interrupt + USER_INTERRUPTS_START);

        interrupt + USER_INTERRUPTS_START
    }
}

pub fn init_interrupts() {
    INTERRUPTS.run_with_mut(|idt| {
        idt.init();
    });
}

// All Types of interrupt handlers
pub trait InterruptHandler {
    fn set_handler(val: Self) -> u8;
}

impl InterruptHandler for BasicInterruptHandler {
    fn set_handler(handler: Self) -> u8 {
        INTERRUPTS.lock().allocate_basic_user_interrupt(handler)
    }
}

impl InterruptHandler for InterruptHandlerWithAllState {
    fn set_handler(handler: Self) -> u8 {
        INTERRUPTS.lock().allocate_user_interrupt_all_saved(handler)
    }
}

/// Puts the handler in the IDT and returns the interrupt/vector number
pub(super) fn allocate_user_interrupt<F: InterruptHandler>(handler: F) -> u8 {
    F::set_handler(handler)
}

/// Puts the handler in the IDT and returns the interrupt/vector number
pub(super) fn allocate_basic_user_interrupt(handler: BasicInterruptHandler) -> u8 {
    BasicInterruptHandler::set_handler(handler)
}

#[allow(dead_code)]
/// Puts the handler in the IDT and returns the interrupt/vector number
pub(super) fn allocate_user_interrupt_all_saved(handler: InterruptHandlerWithAllState) -> u8 {
    InterruptHandlerWithAllState::set_handler(handler)
}

pub fn create_scheduler_interrupt(handler: InterruptHandlerWithAllState) {
    let mut interrupts = INTERRUPTS.lock();
    interrupts.idt.user_defined[SPECIAL_SCHEDULER_INTERRUPT as usize]
        .set_handler_with_number(handler, SPECIAL_SCHEDULER_INTERRUPT + USER_INTERRUPTS_START);
}

pub fn create_syscall_interrupt(handler: InterruptHandlerWithAllState) {
    let mut interrupts = INTERRUPTS.lock();
    interrupts.idt.user_defined[SPECIAL_SYSCALL_INTERRUPT as usize]
        .set_handler_with_number(handler, SPECIAL_SYSCALL_INTERRUPT + USER_INTERRUPTS_START)
        .set_privilege_level(USER_RING)
        .set_disable_interrupts(true)
        .set_stack_index(Some(stack_index::SYSCALL_STACK));
}
