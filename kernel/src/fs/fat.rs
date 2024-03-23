use core::{fmt, mem};

use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};

use crate::{
    devices::ide::IdeDevice, io::NoDebug, memory_management::memory_layout::align_up,
    sync::spin::mutex::Mutex,
};

use super::{DirTreverse, FileAttributes, FileSystem, FileSystemError, INode};

const DIRECTORY_ENTRY_SIZE: u32 = 32;

fn file_attribute_from_fat(attributes: u8) -> FileAttributes {
    FileAttributes {
        read_only: attributes & attrs::READ_ONLY == attrs::READ_ONLY,
        hidden: attributes & attrs::HIDDEN == attrs::HIDDEN,
        system: attributes & attrs::SYSTEM == attrs::SYSTEM,
        volume_label: attributes & attrs::VOLUME_ID == attrs::VOLUME_ID,
        directory: attributes & attrs::DIRECTORY == attrs::DIRECTORY,
        archive: attributes & attrs::ARCHIVE == attrs::ARCHIVE,
    }
}

#[derive(Debug)]
pub enum FatError {
    InvalidBootSector,
    UnexpectedFatEntry,
}

impl From<FatError> for FileSystemError {
    fn from(e: FatError) -> Self {
        FileSystemError::FatError(e)
    }
}

pub fn load_fat_filesystem(
    device: Arc<IdeDevice>,
    start_lba: u32,
    size_in_sectors: u32,
) -> Result<FatFilesystem, FileSystemError> {
    let size = align_up(
        mem::size_of::<FatBootSectorRaw>(),
        device.sector_size() as usize,
    );
    let mut sectors = vec![0; size];

    device
        .read_sync(start_lba as u64, &mut sectors)
        .map_err(|e| FileSystemError::DiskReadError {
            sector: start_lba as u64,
            error: e,
        })?;

    // SAFETY: This is a valid allocated memory
    let boot_sector = unsafe { sectors.as_ptr().cast::<FatBootSectorRaw>().read() };
    let boot_sector = FatBootSector::new(boot_sector, size_in_sectors)?;

    FatFilesystem::new(start_lba, size_in_sectors, boot_sector, device)
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct Fat12_16ExtendedBootSector {
    drive_number: u8,
    reserved: u8,
    boot_signature: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    file_system_type: [u8; 8],
    boot_code: NoDebug<[u8; 448]>,
    boot_signature_2: u16,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
struct Fat32ExtendedBootSector {
    fat_size_32: u32,
    ext_flags: u16,
    fs_version: u16,
    root_cluster: u32,
    fs_info: u16,
    backup_boot_sector: u16,
    reserved: [u8; 12],
    drive_number: u8,
    reserved_2: u8,
    boot_signature: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    file_system_type: [u8; 8],
    boot_code: NoDebug<[u8; 420]>,
    boot_signature_2: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
union FatExtendedBootSector {
    fat12_16: Fat12_16ExtendedBootSector,
    fat32: Fat32ExtendedBootSector,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct FatBootSectorRaw {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors_count: u16,
    number_of_fats: u8,
    root_entry_count: u16,
    total_sectors_16: u16,
    media_type: u8,
    fat_size_16: u16,
    sectors_per_track: u16,
    number_of_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    extended: FatExtendedBootSector,
}

impl fmt::Debug for FatBootSectorRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes_per_sector = self.bytes_per_sector;
        let reserved_sectors_count = self.reserved_sectors_count;
        let root_entry_count = self.root_entry_count;
        let total_sectors_16 = self.total_sectors_16;
        let fat_size_16 = self.fat_size_16;
        let sectors_per_track = self.sectors_per_track;
        let number_of_heads = self.number_of_heads;
        let hidden_sectors = self.hidden_sectors;
        let total_sectors_32 = self.total_sectors_32;

        let is_fat32 = self.fat_size_16 == 0;

        let mut s = f.debug_struct("FatBootSector");

        s.field("jmp_boot", &self.jmp_boot)
            .field("oem_name", &self.oem_name)
            .field("bytes_per_sector", &bytes_per_sector)
            .field("sectors_per_cluster", &self.sectors_per_cluster)
            .field("reserved_sectors_count", &reserved_sectors_count)
            .field("number_of_fats", &self.number_of_fats)
            .field("root_entry_count", &root_entry_count)
            .field("total_sectors_16", &total_sectors_16)
            .field("media_type", &self.media_type)
            .field("fat_size_16", &fat_size_16)
            .field("sectors_per_track", &sectors_per_track)
            .field("number_of_heads", &number_of_heads)
            .field("hidden_sectors", &hidden_sectors)
            .field("total_sectors_32", &total_sectors_32);

        if is_fat32 {
            s.field("extended_fat32", unsafe { &self.extended.fat32 })
                .finish()
        } else {
            s.field("extended_fat12_16", unsafe { &self.extended.fat12_16 })
                .finish()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FatEntry {
    Free,
    // In use, and point to the next cluster
    Next(u32),
    // In use, and this is the last cluster
    EndOfChain,
    Bad,
    Reserved,
}

impl FatEntry {
    pub fn from_u32(ty: FatType, entry: u32) -> FatEntry {
        match ty {
            FatType::Fat12 => {
                if entry == 0 {
                    FatEntry::Free
                } else if entry >= 0xFF8 {
                    FatEntry::EndOfChain
                } else if entry == 0xFF7 {
                    FatEntry::Bad
                } else if (0x002..=0xFF6).contains(&entry) {
                    FatEntry::Next(entry)
                } else {
                    FatEntry::Reserved
                }
            }
            FatType::Fat16 => {
                if entry == 0 {
                    FatEntry::Free
                } else if entry >= 0xFFF8 {
                    FatEntry::EndOfChain
                } else if entry == 0xFFF7 {
                    FatEntry::Bad
                } else if (0x002..=0xFFF6).contains(&entry) {
                    FatEntry::Next(entry)
                } else {
                    FatEntry::Reserved
                }
            }
            FatType::Fat32 => {
                if entry == 0 {
                    FatEntry::Free
                } else if entry >= 0x0FFF_FFF8 {
                    FatEntry::EndOfChain
                } else if entry == 0x0FFF_FFF7 {
                    FatEntry::Bad
                } else if (0x002..=0x0FFF_FFF6).contains(&entry) {
                    FatEntry::Next(entry)
                } else {
                    FatEntry::Reserved
                }
            }
        }
    }
}

#[derive(Debug)]
struct FatBootSector {
    ty: FatType,
    boot_sector: FatBootSectorRaw,
}

#[allow(dead_code)]
impl FatBootSector {
    fn new(boot_sector: FatBootSectorRaw, size_in_sectors: u32) -> Result<FatBootSector, FatError> {
        if unsafe { boot_sector.extended.fat32.boot_signature_2 } != 0xAA55 {
            return Err(FatError::InvalidBootSector);
        }

        let count_of_clusters = size_in_sectors / boot_sector.sectors_per_cluster as u32;

        let fat_type = match count_of_clusters {
            _ if boot_sector.fat_size_16 == 0 => FatType::Fat32,
            0..=4084 => FatType::Fat12,
            4085..=65524 => FatType::Fat16,
            _ => FatType::Fat32,
        };

        Ok(FatBootSector {
            ty: fat_type,
            boot_sector,
        })
    }

    pub fn bytes_per_sector(&self) -> u16 {
        self.boot_sector.bytes_per_sector
    }

    pub fn sectors_per_cluster(&self) -> u8 {
        self.boot_sector.sectors_per_cluster
    }

    pub fn bytes_per_cluster(&self) -> u32 {
        self.boot_sector.sectors_per_cluster as u32 * self.boot_sector.bytes_per_sector as u32
    }

    pub fn reserved_sectors_count(&self) -> u16 {
        self.boot_sector.reserved_sectors_count
    }

    pub fn total_sectors(&self) -> u32 {
        if self.boot_sector.total_sectors_16 != 0 {
            self.boot_sector.total_sectors_16 as u32
        } else {
            self.boot_sector.total_sectors_32
        }
    }

    pub fn fat_size_in_sectors(&self) -> u32 {
        if self.ty == FatType::Fat32 {
            unsafe { self.boot_sector.extended.fat32.fat_size_32 }
        } else {
            self.boot_sector.fat_size_16 as u32
        }
    }

    pub fn number_of_fats(&self) -> u8 {
        self.boot_sector.number_of_fats
    }

    pub fn fat_start_sector(&self) -> u32 {
        self.boot_sector.reserved_sectors_count as u32
    }

    pub fn root_dir_sectors(&self) -> u32 {
        ((self.boot_sector.root_entry_count as u32 * DIRECTORY_ENTRY_SIZE)
            + (self.boot_sector.bytes_per_sector as u32 - 1))
            / self.boot_sector.bytes_per_sector as u32
    }

    pub fn root_dir_start_sector(&self) -> u32 {
        self.fat_start_sector() + self.number_of_fats() as u32 * self.fat_size_in_sectors()
    }

    pub fn data_start_sector(&self) -> u32 {
        self.root_dir_start_sector() + self.root_dir_sectors()
    }

    pub fn data_sectors(&self) -> u32 {
        self.total_sectors() - self.data_start_sector()
    }

    pub fn volume_label(&self) -> &[u8; 11] {
        match self.ty {
            FatType::Fat12 | FatType::Fat16 => unsafe {
                &self.boot_sector.extended.fat12_16.volume_label
            },
            FatType::Fat32 => unsafe { &self.boot_sector.extended.fat32.volume_label },
        }
    }
}

#[allow(dead_code)]
mod attrs {
    pub const READ_ONLY: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const SYSTEM: u8 = 0x04;
    pub const VOLUME_ID: u8 = 0x08;
    pub const DIRECTORY: u8 = 0x10;
    pub const ARCHIVE: u8 = 0x20;
    pub const LONG_NAME: u8 = READ_ONLY | HIDDEN | SYSTEM | VOLUME_ID;
}

#[derive(Debug, Clone)]
enum Directory {
    RootFat12_16 {
        start_sector: u32,
        size_in_sectors: u32,
    },
    Normal {
        inode: INode,
    },
}

pub struct DirectoryIterator<'a> {
    dir: Directory,
    filesystem: &'a FatFilesystem,
    // only hold one sector
    current_sector: Vec<u8>,
    current_sector_index: u32,
    current_cluster: u32,
    entry_index_in_sector: u32,
}

impl DirectoryIterator<'_> {
    fn new(
        filesystem: &FatFilesystem,
        dir: Directory,
    ) -> Result<DirectoryIterator, FileSystemError> {
        let (sector_index, current_cluster, current_sector) = match dir {
            Directory::RootFat12_16 { start_sector, .. } => {
                (start_sector, 0, filesystem.read_sectors(start_sector, 1)?)
            }
            Directory::Normal { ref inode } => {
                if matches!(filesystem.fat_type(), FatType::Fat12 | FatType::Fat16)
                    && inode.start_cluster == 0
                {
                    // looks like we got back using `..` to the root, thus, we should use the root directly
                    return Self::new(filesystem, filesystem.open_root_dir()?);
                }

                let start_sector = filesystem.first_sector_of_cluster(inode.start_cluster as u32);

                (
                    start_sector,
                    inode.start_cluster as u32,
                    filesystem.read_sectors(start_sector, 1)?,
                )
            }
        };
        Ok(DirectoryIterator {
            dir,
            filesystem,
            current_sector,
            current_cluster,
            current_sector_index: sector_index,
            entry_index_in_sector: 0,
        })
    }

    // return true if we got more sectors and we can continue
    fn next_sector(&mut self) -> Result<bool, FileSystemError> {
        // are we done?
        let mut next_sector_index = self.current_sector_index + 1;
        match self.dir {
            Directory::RootFat12_16 {
                start_sector,
                size_in_sectors,
            } => {
                if next_sector_index >= start_sector + size_in_sectors {
                    return Ok(false);
                }
            }
            Directory::Normal { .. } => {
                // did we exceed cluster boundary?
                if next_sector_index % self.filesystem.boot_sector.sectors_per_cluster() as u32 == 0
                {
                    // get next cluster
                    let next_cluster = self.filesystem.next_cluster(self.current_cluster);
                    match next_cluster {
                        Ok(Some(cluster)) => {
                            self.current_cluster = cluster;
                            next_sector_index =
                                cluster * self.filesystem.boot_sector.sectors_per_cluster() as u32;
                        }
                        Ok(None) => {
                            return Ok(false);
                        }
                        Err(_e) => {
                            return Err(FileSystemError::FileNotFound);
                        }
                    }
                }
            }
        }

        self.current_sector = self.filesystem.read_sectors(next_sector_index, 1)?;
        self.current_sector_index = next_sector_index;
        self.entry_index_in_sector = 0;
        Ok(true)
    }

    fn get_next_entry(&mut self) -> Result<&[u8], FileSystemError> {
        let entry_start = self.entry_index_in_sector as usize * DIRECTORY_ENTRY_SIZE as usize;
        let entry_end = entry_start + DIRECTORY_ENTRY_SIZE as usize;
        if entry_end > self.current_sector.len() {
            // we need to read the next sector
            return if self.next_sector()? {
                self.get_next_entry()
            } else {
                Err(FileSystemError::FileNotFound)
            };
        }
        let entry = &self.current_sector[entry_start..entry_end];
        self.entry_index_in_sector += 1;

        assert!(entry.len() == DIRECTORY_ENTRY_SIZE as usize);
        Ok(entry)
    }
}

impl Iterator for DirectoryIterator<'_> {
    type Item = INode;

    fn next(&mut self) -> Option<Self::Item> {
        let mut entry = self.get_next_entry().ok()?;

        loop {
            match entry[0] {
                0x00 => {
                    // this is free and all others are free too, so stop
                    return None;
                }
                0xE5 => {
                    // this is free, get next one
                    entry = self.get_next_entry().ok()?;
                }
                _ => break,
            }
        }
        let mut attributes = entry[11];

        let name = if attributes & attrs::LONG_NAME == attrs::LONG_NAME {
            // long file name
            // this should be the last
            assert!(entry[0] & 0x40 == 0x40);
            let number_of_entries = entry[0] & 0x3F;
            let mut long_name_enteries = Vec::with_capacity(number_of_entries as usize);
            // skip all long file name entries
            for _ in 0..number_of_entries {
                // get the multiple parts
                let name1 = &entry[1..11];
                let name2 = &entry[14..26];
                let name3 = &entry[28..32];

                // construct an iterator and fill the string part
                let name_iter = name1
                    .chunks(2)
                    .chain(name2.chunks(2))
                    .chain(name3.chunks(2))
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|c| c != &0);

                let mut name_part = String::with_capacity(13);
                char::decode_utf16(name_iter)
                    .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .for_each(|c| name_part.push(c));

                // add to the entries
                long_name_enteries.push(name_part);

                // next entry
                entry = self.get_next_entry().ok()?;
            }
            attributes = entry[11];
            let mut name = String::new();
            long_name_enteries
                .into_iter()
                .rev()
                .for_each(|s| name.push_str(&s));
            name
        } else {
            // short file name
            let base_name = &entry[0..8];
            let base_name_end = 8 - base_name.iter().rev().position(|&c| c != 0x20).unwrap();
            let extension = &entry[8..11];

            let mut name = String::with_capacity(13);
            let mut i = 0;
            while i < base_name_end {
                name.push(base_name[i] as char);
                i += 1;
            }
            let extension_present = extension[0] != 0x20;
            if extension_present {
                name.push('.');
                i = 0;
                while i < extension.len() && extension[i] != 0x20 {
                    name.push(extension[i] as char);
                    i += 1;
                }
            }
            name
        };

        let cluster_hi = unsafe {
            let ptr = entry.as_ptr().add(20) as *const u16;
            u16::from_le(*(ptr)) as u32
        };
        let cluster_lo = unsafe {
            let ptr = entry.as_ptr().add(26) as *const u16;
            u16::from_le(*(ptr)) as u32
        };
        let size = unsafe {
            let ptr = entry.as_ptr().add(28) as *const u32;
            u32::from_le(*(ptr))
        };

        let start_cluster = (cluster_hi << 16) | cluster_lo;

        let inode = INode::new_file(
            name,
            file_attribute_from_fat(attributes),
            start_cluster as u64,
            size as u64,
        );

        Some(inode)
    }
}

#[derive(Debug)]
pub struct FatFilesystem {
    start_lba: u32,
    #[allow(dead_code)]
    size_in_sectors: u32,
    boot_sector: Box<FatBootSector>,
    fat: NoDebug<Vec<u8>>,
    device: NoDebug<Arc<IdeDevice>>,
}

impl FatFilesystem {
    fn new(
        start_lba: u32,
        size_in_sectors: u32,
        boot_sector: FatBootSector,
        device: Arc<IdeDevice>,
    ) -> Result<Self, FileSystemError> {
        let mut s = FatFilesystem {
            start_lba,
            size_in_sectors,
            boot_sector: Box::new(boot_sector),
            fat: NoDebug(Vec::new()),
            device: NoDebug(device),
        };

        // TODO: replace by lazily reading FAT when needed
        s.load_fat()?;

        Ok(s)
    }

    pub fn volume_label(&self) -> String {
        let label = self.boot_sector.volume_label();
        let mut label = String::from_utf8_lossy(label).to_string();
        label.retain(|c| c != '\0');
        label
    }

    pub fn fat_type(&self) -> FatType {
        self.boot_sector.ty
    }

    fn first_sector_of_cluster(&self, cluster: u32) -> u32 {
        self.boot_sector.data_start_sector()
            + (cluster - 2) * self.boot_sector.sectors_per_cluster() as u32
    }

    fn read_sectors(&self, start_sector: u32, count: u32) -> Result<Vec<u8>, FileSystemError> {
        let sector_size = self.boot_sector.bytes_per_sector() as usize;
        let mut sectors = vec![0; sector_size * count as usize];

        self.device
            .read_sync((self.start_lba + start_sector) as u64, &mut sectors)
            .map_err(|e| FileSystemError::DiskReadError {
                sector: (self.start_lba + start_sector) as u64,
                error: e,
            })?;

        Ok(sectors)
    }

    fn load_fat(&mut self) -> Result<(), FileSystemError> {
        // already loaded
        assert!(self.fat.is_empty(), "FAT already loaded");

        let fats_size_in_sectors =
            self.boot_sector.fat_size_in_sectors() * self.boot_sector.number_of_fats() as u32;
        let fat_start_sector = self.boot_sector.fat_start_sector();

        self.fat.0 = self.read_sectors(fat_start_sector, fats_size_in_sectors)?;

        Ok(())
    }

    fn read_fat_entry(&self, entry: u32) -> FatEntry {
        let fat_offset = match self.fat_type() {
            FatType::Fat12 => entry * 3 / 2,
            FatType::Fat16 => entry * 2,
            FatType::Fat32 => entry * 4,
        } as usize;
        assert!(fat_offset < self.fat.0.len(), "FAT entry out of bounds");
        let ptr = unsafe { self.fat.0.as_ptr().add(fat_offset) };

        let entry = match self.fat_type() {
            FatType::Fat12 => {
                let byte1 = self.fat.0[fat_offset];
                let byte2 = self.fat.0[fat_offset + 1];
                if entry & 1 == 1 {
                    ((byte2 as u32) << 4) | ((byte1 as u32) >> 4)
                } else {
                    (((byte2 as u32) & 0xF) << 8) | (byte1 as u32)
                }
            }
            FatType::Fat16 => unsafe { (*(ptr as *const u16)) as u32 },
            FatType::Fat32 => unsafe { (*(ptr as *const u32)) & 0x0FFF_FFFF },
        };

        FatEntry::from_u32(self.fat_type(), entry)
    }

    fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FileSystemError> {
        match self.read_fat_entry(cluster) {
            FatEntry::Next(next_cluster) => Ok(Some(next_cluster)),
            FatEntry::EndOfChain => Ok(None),
            FatEntry::Bad => Err(FatError::UnexpectedFatEntry.into()),
            FatEntry::Reserved => Err(FatError::UnexpectedFatEntry.into()),
            FatEntry::Free => Err(FatError::UnexpectedFatEntry.into()),
        }
    }

    fn open_root_dir(&self) -> Result<Directory, FileSystemError> {
        match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => Ok(Directory::RootFat12_16 {
                start_sector: self.boot_sector.root_dir_start_sector(),
                size_in_sectors: self.boot_sector.root_dir_sectors(),
            }),
            FatType::Fat32 => {
                let root_cluster =
                    unsafe { self.boot_sector.boot_sector.extended.fat32.root_cluster };
                let inode = INode::new_file(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    root_cluster as u64,
                    0,
                );
                Ok(Directory::Normal { inode })
            }
        }
    }

    fn open_root_dir_inode(&self) -> Result<INode, FileSystemError> {
        match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // use a special inode for root
                let inode = INode::new_file(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    self.boot_sector.root_dir_start_sector() as u64,
                    self.boot_sector.root_dir_sectors() as u64
                        * self.boot_sector.bytes_per_sector() as u64,
                );

                Ok(inode)
            }
            FatType::Fat32 => {
                let root_cluster =
                    unsafe { self.boot_sector.boot_sector.extended.fat32.root_cluster };
                let inode = INode::new_file(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    root_cluster as u64,
                    0,
                );
                Ok(inode)
            }
        }
    }

    pub fn open_dir_inode(&self, inode: &INode) -> Result<DirectoryIterator, FileSystemError> {
        if !inode.is_dir() {
            return Err(FileSystemError::IsNotDirectory);
        }

        let dir = match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // try to see if this is the root
                // this could be 0 if we are back from using `..` to the root
                if inode.start_cluster() == 0
                    || inode.start_cluster() == self.boot_sector.root_dir_start_sector() as u64
                        && inode.size()
                            == self.boot_sector.root_dir_sectors() as u64
                                * self.boot_sector.bytes_per_sector() as u64
                {
                    self.open_root_dir()?
                } else {
                    Directory::Normal {
                        inode: inode.clone(),
                    }
                }
            }
            FatType::Fat32 => {
                assert!(inode.size() == 0);
                Directory::Normal {
                    inode: inode.clone(),
                }
            }
        };

        DirectoryIterator::new(self, dir)
    }

    pub fn read_file(
        &self,
        inode: &INode,
        position: u32,
        buf: &mut [u8],
    ) -> Result<u64, FileSystemError> {
        if inode.is_dir() {
            return Err(FileSystemError::IsDirectory);
        }
        if position >= inode.size as u32 {
            return Ok(0);
        }
        let remaining_file = inode.size as u32 - position;
        let max_to_read = (buf.len() as u32).min(remaining_file);

        let mut cluster = inode.start_cluster as u32;
        let cluster_index = position / self.boot_sector.bytes_per_cluster();
        for _ in 0..cluster_index {
            cluster = self
                .next_cluster(cluster)?
                .ok_or(FatError::UnexpectedFatEntry)?;
        }

        let mut read = 0;
        let mut position_in_cluster = position % self.boot_sector.bytes_per_cluster();
        while read < max_to_read as usize {
            let cluster_start_sector = self.first_sector_of_cluster(cluster);
            let cluster_offset = position_in_cluster / self.boot_sector.bytes_per_sector() as u32;

            let sector_number = cluster_start_sector + cluster_offset;
            let sector_offset = position_in_cluster % self.boot_sector.bytes_per_sector() as u32;
            let sector_offset = sector_offset as usize;

            let sector = self.read_sectors(sector_number, 1)?;
            let sector = &sector[sector_offset..];

            let to_read = core::cmp::min(sector.len() as u32, max_to_read - read as u32) as usize;
            buf[read..read + to_read].copy_from_slice(&sector[..to_read]);

            read += to_read;
            position_in_cluster += to_read as u32;
            if position_in_cluster >= self.boot_sector.bytes_per_cluster() {
                position_in_cluster = 0;
                cluster = match self.next_cluster(cluster)? {
                    Some(next_cluster) => next_cluster,
                    None => break,
                };
            }
        }

        Ok(read as u64)
    }
}

impl FileSystem for Mutex<FatFilesystem> {
    fn read_file(
        &self,
        inode: &INode,
        position: u64,
        buf: &mut [u8],
    ) -> Result<u64, FileSystemError> {
        assert!(position <= u32::MAX as u64);
        self.lock().read_file(inode, position as u32, buf)
    }

    fn open_root(&self) -> Result<INode, FileSystemError> {
        self.lock().open_root_dir_inode()
    }

    fn read_dir(
        &self,
        inode: &INode,
        handler: &mut dyn FnMut(INode) -> DirTreverse,
    ) -> Result<(), FileSystemError> {
        for node in self.lock().open_dir_inode(inode)? {
            if let DirTreverse::Stop = handler(node) {
                break;
            }
        }

        Ok(())
    }
}
