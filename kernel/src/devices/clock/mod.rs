mod hpet;
mod rtc;

use alloc::sync::Arc;

use crate::{
    bios::tables::{self, BiosTables, Facp},
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::{hpet::Hpet, rtc::Rtc};

// hpet clock for now
static HPET_CLOCK: OnceLock<Option<Arc<Mutex<Hpet>>>> = OnceLock::new();

pub fn init(bios_tables: &BiosTables) {
    let facp = bios_tables.rsdt.get_table::<Facp>();

    let century_reg = facp.map(|facp| facp.century);
    // TODO: use it later, and provide it to everyone who need it
    let rtc_time = Rtc::new(century_reg).get_time();
    println!("Time now: {rtc_time}: UTC");

    let hpet = bios_tables
        .rsdt
        .get_table::<tables::Hpet>()
        .and_then(Hpet::initialize_from_bios_table)
        .map(|hpet| Arc::new(Mutex::new(hpet)));
    HPET_CLOCK.set(hpet).expect("clock already initialized");
}
