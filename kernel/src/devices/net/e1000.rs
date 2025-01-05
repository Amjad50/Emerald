mod desc;

use core::mem;

use alloc::{collections::vec_deque::VecDeque, sync::Arc, vec::Vec};
use desc::{Descriptor, DmaRing, ReceiveDescriptor, TransmitDescriptor};
use tracing::{info, trace, warn};

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    devices::pci::PciDeviceConfig,
    memory_management::virtual_space::VirtualSpace,
    net::{NetworkError, NetworkPacket},
    sync::{once::OnceLock, spin::mutex::Mutex},
    utils::{
        vcell::{RO, RW, WO},
        Pad,
    },
};

use super::{MacAddress, NetworkDevice};

static E1000: OnceLock<Arc<Mutex<E1000>>> = OnceLock::new();

pub fn get_device() -> Option<&'static dyn NetworkDevice> {
    E1000.try_get().map(|e1000| e1000 as &dyn NetworkDevice)
}

#[allow(dead_code)]
#[allow(clippy::identity_op)]
#[allow(clippy::eq_op)]
pub mod flags {
    // EEPROM
    pub const EERD_ADDR_SHIFT: u32 = 8;
    pub const EERD_DATA_SHIFT: u32 = 16;
    pub const EERD_START: u32 = 1 << 0;
    pub const EERD_DONE: u32 = 1 << 4;
    pub const EE_SIZE: u32 = 1 << 9;

    // Control
    pub const CTRL_FD: u32 = 1 << 0;
    pub const CTRL_SPEED_10MB: u32 = 0 << 8;
    pub const CTRL_SPEED_100MB: u32 = 1 << 8;
    pub const CTRL_SPEED_1000MB: u32 = 2 << 8;
    pub const CRTL_FORCE_DPLX: u32 = 1 << 12;

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

    // Transmit Control
    pub const TCTL_EN: u32 = 1 << 1;
    pub const TCTL_PSP: u32 = 1 << 3;
    // collision threshold
    pub const TCTL_CT_SHIFT: u32 = 4;
    pub const TCTL_CT_MASK: u32 = 0xF << TCTL_CT_SHIFT;
    // collision distance
    pub const TCTL_COLD_SHIFT: u32 = 12;
    pub const TCTL_COLD_MASK: u32 = 0x3F << TCTL_COLD_SHIFT;

    // Interrupts
    pub const I_TXDW: u32 = 1 << 0;
    pub const I_TXQE: u32 = 1 << 1;
    pub const I_LSC: u32 = 1 << 2;
    pub const I_RXSEQ: u32 = 1 << 3;
    pub const I_RXDMT0: u32 = 1 << 4;
    pub const I_RXO: u32 = 1 << 6;
    pub const I_RXT0: u32 = 1 << 7;
    pub const I_TXD_LOW: u32 = 1 << 15;
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
    _pad11: Pad<0x27C>,
    transmit_control: RW<u32>,
    _pad12: Pad<0x9FC>,
    led_control: RW<u32>,
    _pad13: Pad<0x160C>,
    receive_data_fifo_head: RW<u32>,
    _pad14: Pad<0x4>,
    receive_data_fifo_tail: RW<u32>,
    _pad15: Pad<0x4>,
    receive_data_fifo_head_saved: RW<u32>,
    _pad16: Pad<0x4>,
    receive_data_fifo_tail_saved: RW<u32>,
    _pad17: Pad<0x4>,
    receive_data_fifo_packet_count: RW<u32>,
    _pad18: Pad<0x3CC>,
    receive_descriptor_base_low: RW<u32>,
    receive_descriptor_base_high: RW<u32>,
    receive_descriptor_length: RW<u32>,
    _pad19: Pad<0x4>,
    receive_descriptor_head: RW<u32>,
    _pad20: Pad<0x4>,
    receive_descriptor_tail: RW<u32>,
    _pad21: Pad<0x4>,
    receive_delay_timer: RW<u32>,
    _pad22: Pad<0x8>,
    receive_interrupt_abs_delay_timer: RW<u32>,
    _pad23: Pad<0xFD0>,
    transmit_descriptor_base_low: RW<u32>,
    transmit_descriptor_base_high: RW<u32>,
    transmit_descriptor_length: RW<u32>,
    _pad24: Pad<0x4>,
    transmit_descriptor_head: RW<u32>,
    _pad25: Pad<0x4>,
    transmit_descriptor_tail: RW<u32>,
    _pad26: Pad<0x4>,
    transmit_descriptor_interrupt_delay: RW<u32>,
    _pad27: Pad<0x19DC>,
    multicast_table_array: [RW<u32>; 128],
    receive_addresses: [(RW<u32>, RW<u32>); 16],
}

struct E1000 {
    mmio: VirtualSpace<E1000Mmio>,
    eeprom_size: u16,

    recv_ring: DmaRing<ReceiveDescriptor, 128>,
    transmit_ring: DmaRing<TransmitDescriptor, 128>,

    received_queue: VecDeque<Vec<u8>>,
    in_middle_of_packet: bool,
}

#[allow(dead_code)]
impl E1000 {
    fn new(mmio: VirtualSpace<E1000Mmio>) -> Self {
        let eecd = mmio.eecd.read();
        let eeprom_size = if eecd & flags::EE_SIZE != 0 { 256 } else { 64 };

        let mut recv_ring = DmaRing::new();
        recv_ring.allocate_all_for_hw();

        Self {
            mmio,
            eeprom_size,
            recv_ring,
            transmit_ring: DmaRing::new(),
            received_queue: VecDeque::new(),
            in_middle_of_packet: false,
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
            assert_eq!(self.recv_ring.physical_ptr() & 0xF, 0);
            self.mmio
                .receive_descriptor_base_low
                .write(self.recv_ring.physical_ptr() as u32);
            self.mmio
                .receive_descriptor_base_high
                .write((self.recv_ring.physical_ptr() >> 32) as u32);
            self.mmio
                .receive_descriptor_length
                .write(self.recv_ring.bytes_len() as u32);
            self.mmio
                .receive_descriptor_head
                .write(self.recv_ring.head() as u32);
            self.mmio
                .receive_descriptor_tail
                .write(self.recv_ring.tail() as u32);

            self.mmio.receive_delay_timer.write(0);
            self.mmio.receive_interrupt_abs_delay_timer.write(0);

            for i in 0..128 {
                self.mmio.multicast_table_array[i].write(0);
            }

            // Enable
            self.mmio.receive_control.write(
                flags::RCTL_EN
                    | flags::RCTL_LPE
                    | flags::RCTL_BAM
                    | flags::RCTL_BSIZE_4096
                    | flags::RCTL_STRIP_ETH_CRC,
            )
        };
    }

    pub fn init_transmit(&self) {
        // 14.5 Transmit Initialization
        unsafe {
            assert_eq!(self.recv_ring.physical_ptr() & 0xF, 0);
            self.mmio
                .transmit_descriptor_base_low
                .write(self.transmit_ring.physical_ptr() as u32);
            self.mmio
                .transmit_descriptor_base_high
                .write((self.transmit_ring.physical_ptr() >> 32) as u32);
            self.mmio
                .transmit_descriptor_length
                .write(self.transmit_ring.bytes_len() as u32);
            self.mmio
                .transmit_descriptor_head
                .write(self.transmit_ring.head() as u32);
            self.mmio
                .transmit_descriptor_tail
                .write(self.transmit_ring.tail() as u32);

            self.mmio.transmit_descriptor_interrupt_delay.write(0);

            self.mmio.transmit_control.write(
                flags::TCTL_EN
                    | flags::TCTL_PSP
                    | (0xF << flags::TCTL_CT_SHIFT)
                    | (0x40 << flags::TCTL_COLD_SHIFT),
            );
        }
    }

    pub fn enable_interrupts(&self) {
        unsafe {
            self.mmio.interrupt_mask_set.write(
                flags::I_LSC
                    | flags::I_RXSEQ
                    | flags::I_RXDMT0
                    | flags::I_RXO
                    | flags::I_RXT0
                    | flags::I_TXDW
                    | flags::I_TXD_LOW,
            );
            // clear any pending interrupts
            self.mmio.interrupt_cause_read.read();
            self.flush_writes();
        }
    }

    pub fn flush_writes(&self) {
        self.mmio.status.read();
    }

    pub fn handle_recv(&mut self) {
        let head = self.mmio.receive_descriptor_head.read() as u16;

        let mut count = 0;
        while let Some(desc) = self.recv_ring.pop_next(head) {
            count += 1;

            if self.in_middle_of_packet {
                self.received_queue
                    .back_mut()
                    .expect("No packet in queue")
                    .extend_from_slice(desc.data());
            } else {
                self.received_queue.push_back(desc.data().to_vec());
            }
            self.in_middle_of_packet = !desc.is_end_of_packet();

            self.recv_ring.allocate_next_for_hw();
        }

        let new_tail = self.recv_ring.tail();
        trace!("Processed {count} descriptors, new tail: {new_tail:x}");
        unsafe { self.mmio.receive_descriptor_tail.write(new_tail as u32) };
    }

    pub fn handle_transmit_interrupt(&mut self) {
        let head = self.mmio.transmit_descriptor_head.read() as u16;
        // just pop all those that are done, so we can allocate them
        // later, no need to do any processing here
        while self.transmit_ring.pop_next(head).is_some() {}
    }

    pub fn transmit_raw(&mut self, data: &[u8]) {
        assert!(data.len() < 4096);

        let Some(desc) = self.transmit_ring.allocate_next_for_hw() else {
            todo!("Transmit queue is full, implement dynamic driver queueing");
        };

        desc.data_mut(data.len()).copy_from_slice(data);
        desc.prepare_for_transmit();

        unsafe {
            self.mmio
                .transmit_descriptor_tail
                .write(self.transmit_ring.tail() as u32)
        };

        self.flush_writes();
    }

    pub fn transmit_packet(&mut self, packet: &NetworkPacket) -> Result<(), NetworkError> {
        if packet.size() > 4096 {
            return Err(NetworkError::PacketTooLarge(packet.size()));
        }

        let Some(desc) = self.transmit_ring.allocate_next_for_hw() else {
            todo!("Transmit queue is full, implement dynamic driver queueing");
        };

        let data = desc.data_mut(packet.size());
        packet.write_into_buffer(data)?;

        desc.prepare_for_transmit();

        unsafe {
            self.mmio
                .transmit_descriptor_tail
                .write(self.transmit_ring.tail() as u32)
        };

        self.flush_writes();

        Ok(())
    }

    pub fn receive_packet(&mut self) -> Option<Vec<u8>> {
        self.received_queue.pop_front()
    }

    // might not work depending on the network card
    pub fn enable_loopback(&self) {
        unsafe {
            self.mmio
                .receive_control
                .write(self.mmio.receive_control.read() | flags::RCTL_LBM_YES);
        }
    }

    pub fn enable_full_duplex(&self) {
        unsafe {
            self.mmio
                .control
                .write(self.mmio.control.read() | flags::CTRL_FD | flags::CRTL_FORCE_DPLX);
        }
    }
}

impl NetworkDevice for Arc<Mutex<E1000>> {
    fn mac_address(&self) -> MacAddress {
        self.lock().read_mac_address()
    }

    fn send(&self, data: &NetworkPacket) -> Result<(), NetworkError> {
        self.lock().transmit_packet(data)
    }

    fn receive_into(&self, packet: &mut NetworkPacket) -> Result<bool, NetworkError> {
        if let Some(data) = self.lock().receive_packet() {
            packet.read_from_buffer(&data)?;
            Ok(true)
        } else {
            Ok(false)
        }
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
        .set(Arc::new(Mutex::new(E1000::new(mmio))))
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

    e1000.enable_interrupts();
    e1000.init_recv();
    e1000.init_transmit();
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
    if cause & flags::I_RXSEQ != 0 {
        // Receiver Sequence Error
        warn!("Receiver Sequence Error");
    }
    if cause & flags::I_TXD_LOW != 0 {
        // Transmit Descriptor Low Ring
        warn!("Transmit Descriptor Low Ring");
    }

    e1000.handle_recv();
    e1000.handle_transmit_interrupt();

    apic::return_from_interrupt();
}
