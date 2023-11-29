use crate::io::NoDebug;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct PartitionEntry {
    pub bootable: u8,
    pub start_head: u8,
    pub start_sector: u8,
    pub start_cylinder: u8,
    pub partition_type: u8,
    pub end_head: u8,
    pub end_sector: u8,
    pub end_cylinder: u8,
    pub start_lba: u32,
    pub size_in_sectors: u32,
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct MbrRaw {
    pub boot_code: NoDebug<[u8; 446]>,
    pub partition_table: [PartitionEntry; 4],
    pub signature: u16,
}

impl MbrRaw {
    pub fn is_valid(&self) -> bool {
        self.signature == 0xAA55
    }
}
