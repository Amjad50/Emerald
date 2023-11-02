use core::sync::atomic::{AtomicBool, Ordering};

use crate::cpu;

pub struct Lock {
    name: &'static str,
    locked: AtomicBool,
    cpu_id: u32,
}

impl Lock {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            locked: AtomicBool::new(false),
            cpu_id: 0,
        }
    }

    pub fn lock(&mut self) {
        if self.are_we_holding() {
            panic!("{}: lock() called again by owner", self.name);
        }
        while self
            .locked
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            cpu::pause!();
        }
        self.cpu_id = cpu::cpu_id();
    }

    pub fn unlock(&mut self) {
        if !self.are_we_holding() {
            panic!("{}: unlock() called by non-owner", self.name);
        }
        self.cpu_id = 0;
        self.locked.store(false, Ordering::Relaxed);
    }

    fn are_we_holding(&self) -> bool {
        self.locked.load(Ordering::Relaxed) && self.cpu_id == cpu::cpu_id()
    }
}
