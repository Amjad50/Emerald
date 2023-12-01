pub mod apic;
mod handlers;

use crate::sync::spin::mutex::Mutex;

use super::idt::{InterruptDescriptorTable, InterruptHandler, InterruptHandlerWithAllState};

static INTERRUPTS: Mutex<Interrupts> = Mutex::new(Interrupts::empty());

const USER_INTERRUPTS_START: u8 = 0x20;
const MAX_USER_INTERRUPTS: u8 = 0xe0 - 0x10;
pub const SPECIAL_SCHEDULER_INTERRUPT: u8 = 0xdf; // last one (0xFF)

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

    fn allocate_user_interrupt(&mut self, handler: InterruptHandler) -> u8 {
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

/// Puts the handler in the IDT and returns the interrupt/vector number
pub(super) fn allocate_user_interrupt(handler: InterruptHandler) -> u8 {
    INTERRUPTS.lock().allocate_user_interrupt(handler)
}

#[allow(dead_code)]
pub(super) fn allocate_user_interrupt_all_saved(handler: InterruptHandlerWithAllState) -> u8 {
    INTERRUPTS.lock().allocate_user_interrupt_all_saved(handler)
}

pub fn create_scheduler_interrupt(handler: InterruptHandlerWithAllState) {
    let mut interrupts = INTERRUPTS.lock();
    interrupts.idt.user_defined[SPECIAL_SCHEDULER_INTERRUPT as usize]
        .set_handler_with_number(handler, SPECIAL_SCHEDULER_INTERRUPT + USER_INTERRUPTS_START);
}
