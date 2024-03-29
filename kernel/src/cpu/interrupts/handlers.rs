//! Global handlers that have several purposes and doesn't belong in 1 place specifically

use crate::{
    cpu::idt::InterruptAllSavedState,
    devices::{clock, keyboard_mouse},
    process::scheduler,
};

use super::apic;

pub extern "cdecl" fn apic_timer_handler(all_state: &mut InterruptAllSavedState) {
    // make sure its initialized
    clock::clocks().tick_system_time();
    // trigger poll if there is any events
    keyboard_mouse::poll_events();

    scheduler::yield_current_if_any(all_state);
    apic::return_from_interrupt();
}
