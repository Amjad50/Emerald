use core::{fmt, mem};

use tracing::{info, trace, warn};

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    devices::pci::PciDeviceConfig,
    memory_management::{
        memory_layout::{physical2virtual, virtual2physical},
        physical_page_allocator,
        virtual_space::VirtualSpace,
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
    utils::{
        vcell::{RO, RW, WO},
        Pad,
    },
};

static E1000: OnceLock<Mutex<E1000>> = OnceLock::new();

#[allow(dead_code)]
#[allow(clippy::identity_op)]
#[allow(clippy::eq_op)]
pub mod flags {
    pub const EERD_ADDR_SHIFT: u32 = 8;
    pub const EERD_DATA_SHIFT: u32 = 16;
    pub const EERD_START: u32 = 1 << 0;
    pub const EERD_DONE: u32 = 1 << 4;
    pub const EE_SIZE: u32 = 1 << 9;

    // Receive Control
    pub const RCTL_EN: u32 = 1 << 1;
    pub const RCTL_SBP: u32 = 1 << 2;
    pub const RCTL_UPE: u32 = 1 << 3;
    pub const RCTL_MPE: u32 = 1 << 4;
    pub const RCTL_LPE: u32 = 1 << 5;
    pub const RCTL_LBM_NO: u32 = 0 << 6;
    pub const RCTL_LBM_YES: u32 = 3 << 6;
    pub const RCTL_RDMTS_HALF: u32 = 0 << 8;
    pub const RCTL_RDMTS_QUARTER: u32 = 1 << 8;
    pub const RCTL_RDMTS_ONE_EIGHTH: u32 = 2 << 8;
    pub const RCTL_MO_36: u32 = 0 << 12;
    pub const RCTL_MO_35: u32 = 1 << 12;
    pub const RCTL_MO_34: u32 = 2 << 12;
    pub const RCTL_MO_33: u32 = 3 << 12;
    pub const RCTL_BAM: u32 = 1 << 15;
    pub const RCTL_BSIZE_256: u32 = 0 << 25 | 3 << 16;
    pub const RCTL_BSIZE_512: u32 = 0 << 25 | 2 << 16;
    pub const RCTL_BSIZE_1024: u32 = 0 << 25 | 1 << 16;
    pub const RCTL_BSIZE_2048: u32 = 0 << 25 | 0 << 16;
    pub const RCTL_BSIZE_4096: u32 = 1 << 25 | 3 << 16;
    pub const RCTL_BSIZE_8192: u32 = 1 << 25 | 2 << 16;
    pub const RCTL_BSIZE_16384: u32 = 1 << 25 | 1 << 16;
    pub const RCTL_VLAN_FILTER_EN: u32 = 1 << 18;
    pub const RCTL_CFI_EN: u32 = 1 << 19;
    pub const RCTL_CFI: u32 = 1 << 20;
    pub const RCTL_DPF: u32 = 1 << 22;
    pub const RCTL_PMCF: u32 = 1 << 23;
    pub const RCTL_STRIP_ETH_CRC: u32 = 1 << 26;

    // Interrupts
    pub const I_TXDW: u32 = 1 << 0;
    pub const I_TXQE: u32 = 1 << 1;
    pub const I_LSC: u32 = 1 << 2;
    pub const I_RXSEQ: u32 = 1 << 3;
    pub const I_RXDMT0: u32 = 1 << 4;
    pub const I_RXO: u32 = 1 << 6;
    pub const I_RXT0: u32 = 1 << 7;
}

#[allow(dead_code)]
mod recv_desc {
    pub const STATUS_DD: u8 = 1 << 0;
    pub const STATUS_EOP: u8 = 1 << 1;
    pub const STATUS_IGNORE_CHECKSUM: u8 = 1 << 2;
    pub const STATUS_VLAN: u8 = 1 << 3;
    pub const STATUS_TCP_CHECKSUM: u8 = 1 << 5;
    pub const STATUS_IP_CHECKSUM: u8 = 1 << 6;
    pub const STATUS_PIF: u8 = 1 << 7;
}

#[allow(dead_code)]
mod pci_cfg {
    // PCI Command
    pub const CMD_IO_SPACE: u16 = 1 << 0;
    pub const CMD_MEM_SPACE: u16 = 1 << 1;
    pub const CMD_BUS_MASTER: u16 = 1 << 2;
}

#[repr(C, align(8))]
struct E1000Mmio {
    control: RW<u32>,
    _pad0: Pad<4>,
    status: RO<u32>,
    _pad1: Pad<4>,
    eecd: RW<u32>,
    eerd: RW<u32>,
    ctrl_ext: RW<u32>,
    flash: RW<u32>,
    mdi_control: RW<u32>,
    _pad2: Pad<4>,
    flow_control_addr_low: RW<u32>,
    flow_control_addr_high: RW<u32>,
    flow_control_type: RW<u32>,
    _pad3: Pad<4>,
    vlan_ethertype: RW<u32>,
    _pad4: Pad<0x82>,
    interrupt_cause_read: RW<u32>,
    interrupt_throttling: RW<u32>,
    interrupt_cause_set: RW<u32>,
    _pad5: Pad<4>,
    interrupt_mask_set: RW<u32>,
    _pad6: Pad<4>,
    interrupt_mask_clear: WO<u32>,
    _pad7: Pad<0x24>,
    receive_control: RW<u32>,
    _pad8: Pad<0x6C>,
    flow_control_transmit_timer: RW<u32>,
    _pad9: Pad<4>,
    transmit_config_word: RW<u32>,
    _pad10: Pad<4>,
    receive_config_word: RO<u32>,
    _pad11: Pad<0xC7C>,
    led_control: RW<u32>,
    _pad12: Pad<0x160C>,
    receive_data_fifo_head: RW<u32>,
    _pad13: Pad<0x4>,
    receive_data_fifo_tail: RW<u32>,
    _pad14: Pad<0x4>,
    receive_data_fifo_head_saved: RW<u32>,
    _pad15: Pad<0x4>,
    receive_data_fifo_tail_saved: RW<u32>,
    _pad16: Pad<0x4>,
    receive_data_fifo_packet_count: RW<u32>,
    _pad17: Pad<0x3CC>,
    receive_descriptor_base_low: RW<u32>,
    receive_descriptor_base_high: RW<u32>,
    receive_descriptor_length: RW<u32>,
    _pad18: Pad<0x4>,
    receive_descriptor_head: RW<u32>,
    _pad19: Pad<0x4>,
    receive_descriptor_tail: RW<u32>,
    _pad20: Pad<0x4>,
    receive_delay_timer: RW<u32>,
    _pad21: Pad<0x8>,
    receive_interrupt_abs_delay_timer: RW<u32>,
    _pad22: Pad<0x29D0>,
    multicast_table_array: [RW<u32>; 128],
    receive_addresses: [(RW<u32>, RW<u32>); 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct ReceiveDescriptor {
    address: u64,
    len: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

impl ReceiveDescriptor {
    #[allow(dead_code)]
    pub fn data(&self) -> &[u8] {
        assert!(self.len <= 4096);
        assert!(self.address != 0);
        unsafe {
            core::slice::from_raw_parts(
                physical2virtual(self.address) as *const u8,
                self.len as usize,
            )
        }
    }
}

struct ReceiveRing<const N: usize> {
    ring: &'static mut [ReceiveDescriptor],
    head: u16,
}

impl<const N: usize> ReceiveRing<N> {
    pub fn new() -> Self {
        assert!(N % 8 == 0); // ring must be multiple of 8
        assert!(N * 16 < 4096); // less than physical page

        let ring: &mut [ReceiveDescriptor] = unsafe {
            core::slice::from_raw_parts_mut(physical_page_allocator::alloc_zeroed().cast(), N)
        };

        // set addresses
        for elem in ring.iter_mut() {
            elem.address =
                virtual2physical(unsafe { physical_page_allocator::alloc_zeroed() } as usize)
        }

        Self { ring, head: 0 }
    }

    pub const fn bytes_len(&self) -> usize {
        N * core::mem::size_of::<ReceiveDescriptor>()
    }

    pub fn physical_ptr(&self) -> u64 {
        virtual2physical(self.ring.as_ptr() as usize)
    }

    pub fn get_tail(&self) -> u16 {
        self.head.wrapping_sub(1) % N as u16
    }

    pub fn get_next_entry(&mut self, hw_head: u16) -> Option<&mut ReceiveDescriptor> {
        // check each entry from where we are at now until where the NIC is

        if hw_head == self.head {
            None
        } else if self.ring[self.head as usize].status & recv_desc::STATUS_DD != 0 {
            let res = Some(&mut self.ring[self.head as usize]);
            self.head = (self.head + 1) % N as u16;
            res
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
struct MacAddress([u8; 6]);

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

struct E1000 {
    mmio: VirtualSpace<E1000Mmio>,
    eeprom_size: u16,

    ring: ReceiveRing<128>,
}

impl E1000 {
    fn new(mmio: VirtualSpace<E1000Mmio>) -> Self {
        let eecd = mmio.eecd.read();
        let eeprom_size = if eecd & flags::EE_SIZE != 0 { 256 } else { 64 };

        let ring = ReceiveRing::new();

        Self {
            mmio,
            eeprom_size,
            ring,
        }
    }

    pub fn read_eeprom(&self, offset: u16) -> u16 {
        assert!(offset < self.eeprom_size);
        assert!(offset <= 0xFF);

        let data = (offset as u32) << flags::EERD_ADDR_SHIFT | flags::EERD_START;
        unsafe { self.mmio.eerd.write(data) };

        while self.mmio.eerd.read() & flags::EERD_DONE == 0 {
            core::hint::spin_loop();
        }

        (self.mmio.eerd.read() >> flags::EERD_DATA_SHIFT) as u16
    }

    pub fn read_mac_address(&self) -> MacAddress {
        let low = self.read_eeprom(0);
        let mid = self.read_eeprom(1);
        let high = self.read_eeprom(2);

        MacAddress([
            (low & 0xFF) as u8,
            (low >> 8) as u8,
            (mid & 0xFF) as u8,
            (mid >> 8) as u8,
            (high & 0xFF) as u8,
            (high >> 8) as u8,
        ])
    }

    pub fn init_recv(&self) {
        // 14.4 Receive Initialization
        unsafe {
            assert_eq!(self.ring.physical_ptr() & 0xF, 0);
            self.mmio
                .receive_descriptor_base_low
                .write(self.ring.physical_ptr() as u32);
            self.mmio
                .receive_descriptor_base_high
                .write((self.ring.physical_ptr() >> 32) as u32);
            self.mmio
                .receive_descriptor_length
                .write(self.ring.bytes_len() as u32);
            self.mmio
                .receive_descriptor_head
                .write(self.ring.head as u32);
            self.mmio
                .receive_descriptor_tail
                .write(self.ring.get_tail() as u32);

            self.mmio.receive_delay_timer.write(0);
            self.mmio.receive_interrupt_abs_delay_timer.write(0);

            for i in 0..128 {
                self.mmio.multicast_table_array[i].write(0);
            }
        }
    }

    pub fn enable_recv(&self) {
        unsafe {
            self.mmio.receive_control.write(
                flags::RCTL_EN
                    | flags::RCTL_LPE
                    | flags::RCTL_BAM
                    | flags::RCTL_BSIZE_4096
                    | flags::RCTL_STRIP_ETH_CRC,
            )
        };
    }

    fn enable_interrupts(&self) {
        unsafe {
            self.mmio.interrupt_mask_set.write(
                flags::I_LSC | flags::I_RXSEQ | flags::I_RXDMT0 | flags::I_RXO | flags::I_RXT0,
            );
            // clear any pending interrupts
            self.mmio.interrupt_cause_read.read();
            self.flush_writes();
        }
    }

    pub fn flush_writes(&self) {
        self.mmio.status.read();
    }

    fn handle_recv(&mut self) {
        let head = self.mmio.receive_descriptor_head.read() as u16;

        let mut count = 0;
        while let Some(desc) = self.ring.get_next_entry(head) {
            count += 1;

            // TODO: do something with the packet
            // println!("{desc:X?}");
            // crate::io::hexdump(desc.data());

            // clear status (not really needed, but maybe some hardware need that?)
            desc.status = 0;
        }

        let new_tail = self.ring.get_tail();

        trace!("Processed {count} descriptors, new tail: {new_tail:x}");

        unsafe { self.mmio.receive_descriptor_tail.write(new_tail as u32) };
    }
}

pub fn try_register(pci_device: &PciDeviceConfig) -> bool {
    match (pci_device.vendor_id, pci_device.device_id) {
        // TODO: this excludes (82541xx and 82547GI/EI)
        //       they have a lot of special differences from the rest
        (
            0x8086,
            0x100E..=0x1012
            | 0x1015..=0x1017
            | 0x101D
            | 0x1026..=0x1028
            | 0x1079..=0x107B
            | 0x1107
            | 0x1112,
        ) => {} // allow
        _ => return false,
    }

    let Some((mem_base, mem_size, _)) = pci_device.base_address[0].get_memory() else {
        warn!("No valid memory base address");
        return false;
    };

    let mut command = pci_device.read_command();
    if command & pci_cfg::CMD_BUS_MASTER == 0 {
        // enable bus master
        command |= pci_cfg::CMD_BUS_MASTER;
        pci_device.write_command(command);
    }

    assert!(mem_size >= mem::size_of::<E1000Mmio>());
    assert_ne!(mem_base, 0);
    assert_eq!(mem_base % 8, 0);

    let mmio =
        unsafe { VirtualSpace::<E1000Mmio>::new(mem_base as u64) }.expect("Failed to map MMIO");
    // set mmio first
    E1000
        .set(Mutex::new(E1000::new(mmio)))
        .ok()
        .expect("Should only be called once");

    // TODO: handle overlapping interrupts correctly
    apic::assign_io_irq(
        interrupt as BasicInterruptHandler,
        pci_device.interrupt_line,
        cpu::cpu(),
    );

    let e1000 = E1000.get().lock();

    info!("MAC address: {:?}", e1000.read_mac_address());

    e1000.init_recv();
    e1000.enable_interrupts();
    e1000.enable_recv();
    e1000.flush_writes();

    true
}

extern "x86-interrupt" fn interrupt(_stack_frame: InterruptStackFrame64) {
    let mut e1000 = E1000.get().lock();
    unsafe { e1000.mmio.interrupt_mask_set.write(0x1) };
    let cause = e1000.mmio.interrupt_cause_read.read();

    if cause & flags::I_RXO != 0 {
        // Receiver FIFO overrun
        warn!("Receiver FIFO overrun");
    }
    if cause & flags::I_LSC != 0 {
        // Link Status Change
        warn!("Link Status Change");
    }

    e1000.handle_recv();

    apic::return_from_interrupt();
}
