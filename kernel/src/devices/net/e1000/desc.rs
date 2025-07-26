use crate::memory_management::{
    memory_layout::{physical2virtual, virtual2physical},
    physical_page_allocator,
};

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
mod transmit_desc {
    pub const CMD_EOP: u8 = 1 << 0;
    pub const CMD_INS_FCS: u8 = 1 << 1;
    pub const CMD_INS_CHECKSUM: u8 = 1 << 2;
    pub const CMD_REPORT_STATUS: u8 = 1 << 3;
    pub const CMD_REPORT_PACKET_SENT: u8 = 1 << 4;
    pub const CMD_VLAN_EN: u8 = 1 << 6;
    pub const CMD_INTERRUPT_DELAY: u8 = 1 << 7;

    pub const STATUS_DD: u8 = 1 << 0;
    pub const STATUS_EXCESS_COLLISIONS: u8 = 1 << 1;
    pub const STATUS_LATE_COLLISION: u8 = 1 << 2;
    pub const STATUS_UNDERRUN: u8 = 1 << 3;
}

pub trait Descriptor {
    fn data(&self) -> &[u8];
    fn reset(&mut self);
    fn init(&mut self);
    fn is_hw_done(&self) -> bool;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ReceiveDescriptor {
    address: u64,
    len: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

impl ReceiveDescriptor {
    pub fn is_end_of_packet(&self) -> bool {
        self.status & recv_desc::STATUS_EOP != 0
    }
}

impl Descriptor for ReceiveDescriptor {
    fn data(&self) -> &[u8] {
        assert!(self.len <= 4096);
        assert!(self.address != 0);
        unsafe {
            core::slice::from_raw_parts(
                physical2virtual(self.address) as *const u8,
                self.len as usize,
            )
        }
    }

    fn reset(&mut self) {
        assert!(self.address != 0);
        self.status = 0;
    }

    fn init(&mut self) {
        self.status = 0;
        self.address =
            virtual2physical(unsafe { physical_page_allocator::alloc_zeroed() } as usize);
    }

    fn is_hw_done(&self) -> bool {
        self.status & recv_desc::STATUS_DD != 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TransmitDescriptor {
    address: u64,
    len: u16,
    checksum_offset: u8,
    cmd: u8,
    status: u8,
    checksum_start: u8,
    special: u16,
}

#[allow(dead_code)]
impl TransmitDescriptor {
    pub fn data_mut(&mut self, len: usize) -> &mut [u8] {
        assert!(len <= 4096);
        assert!(self.address != 0);

        self.len = len as u16;
        unsafe {
            core::slice::from_raw_parts_mut(
                physical2virtual(self.address) as *mut u8,
                self.len as usize,
            )
        }
    }

    pub fn prepare_for_transmit(&mut self) {
        assert!(self.len <= 4096);
        assert!(self.address != 0);

        self.cmd =
            transmit_desc::CMD_EOP | transmit_desc::CMD_REPORT_STATUS | transmit_desc::CMD_INS_FCS;
    }
}

impl Descriptor for TransmitDescriptor {
    fn data(&self) -> &[u8] {
        assert!(self.len <= 4096);
        assert!(self.address != 0);
        unsafe {
            core::slice::from_raw_parts(
                physical2virtual(self.address) as *const u8,
                self.len as usize,
            )
        }
    }

    fn reset(&mut self) {
        assert!(self.address != 0);
        self.status = 0;
    }

    fn init(&mut self) {
        self.status = 0;
        self.address =
            virtual2physical(unsafe { physical_page_allocator::alloc_zeroed() } as usize);
    }

    fn is_hw_done(&self) -> bool {
        self.status & transmit_desc::STATUS_DD != 0
    }
}

pub struct DmaRing<T: Descriptor + 'static, const N: usize> {
    ring: &'static mut [T],
    head: u16,
    tail: u16,
}

#[allow(dead_code)]
impl<T: Descriptor, const N: usize> DmaRing<T, N> {
    pub fn new() -> Self {
        assert!(N.is_multiple_of(8)); // ring must be multiple of 8
        assert!(N * 16 < 4096); // less than physical page

        let ring: &mut [T] = unsafe {
            core::slice::from_raw_parts_mut(physical_page_allocator::alloc_zeroed().cast(), N)
        };

        // set addresses
        for elem in ring.iter_mut() {
            elem.init();
        }

        Self {
            ring,
            head: 0,
            tail: 0,
        }
    }

    pub fn queue_len(&self, hw_head: u16) -> usize {
        hw_head.wrapping_sub(self.tail).wrapping_sub(1) as usize % N
    }

    pub const fn bytes_len(&self) -> usize {
        N * core::mem::size_of::<T>()
    }

    pub fn physical_ptr(&self) -> u64 {
        virtual2physical(self.ring.as_ptr() as usize)
    }

    pub fn head(&self) -> u16 {
        self.head
    }

    pub fn tail(&self) -> u16 {
        self.tail
    }

    pub fn pop_next(&mut self, hw_head: u16) -> Option<&mut T> {
        // check each entry from where we are at now until where the NIC is

        if hw_head == self.head {
            None
        } else if self.ring[self.head as usize].is_hw_done() {
            let res = Some(&mut self.ring[self.head as usize]);
            self.head = (self.head + 1) % N as u16;
            res
        } else {
            None
        }
    }

    pub fn allocate_next_for_hw(&mut self) -> Option<&mut T> {
        // queue is full
        if (self.tail + 1) % N as u16 == self.head {
            None
        } else {
            let res = &mut self.ring[self.tail as usize];
            self.tail = (self.tail + 1) % N as u16;
            res.reset();
            Some(res)
        }
    }

    pub fn allocate_all_for_hw(&mut self) {
        while self.allocate_next_for_hw().is_some() {}
    }
}
