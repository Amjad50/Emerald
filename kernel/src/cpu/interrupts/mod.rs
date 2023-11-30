pub mod apic;

use crate::sync::spin::mutex::Mutex;

use super::idt::{InterruptDescriptorTable, InterruptHandler};

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

    fn allocate_user_interrupt(&mut self, handler: InterruptHandler) -> u8 {
        if self.last_used_user_interrupt >= MAX_USER_INTERRUPTS as u16 {
            panic!("No more user interrupts available");
        }

        let interrupt = self.last_used_user_interrupt;
        self.last_used_user_interrupt += 1;

        self.idt.user_defined[interrupt as usize].set_handler(handler);

        interrupt as u8 + USER_INTERRUPTS_START
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

pub fn create_scheduler_interrupt(handler: InterruptHandler) {
    let mut interrupts = INTERRUPTS.lock();
    interrupts.idt.user_defined[SPECIAL_SCHEDULER_INTERRUPT as usize].set_handler(handler);
}
