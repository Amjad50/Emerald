use crate::sync::spin::mutex::Mutex;

use super::idt::InterruptDescriptorTable;

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::empty();
static mut IDT_LOCK: Mutex<()> = Mutex::new(());

pub fn init_interrupts() {
    let _lock = unsafe { IDT_LOCK.lock() };
    let idt = unsafe { &mut IDT };
    idt.init_default_handlers();

    // TODO: add our custom handlers

    idt.apply();
}
