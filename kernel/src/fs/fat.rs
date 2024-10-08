use core::{cell::Cell, fmt, mem, ops::Range};

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};

use crate::{
    devices::ide::IdeDevice,
    io::NoDebug,
    memory_management::memory_layout::{align_down, align_up},
    sync::spin::mutex::Mutex,
};

use super::{
    AccessHelper, BaseNode, DirTreverse, DirectoryNode, FileAttributes, FileNode, FileSystem,
    FileSystemError, Node,
};

const DIRECTORY_ENTRY_SIZE: u32 = 32;

fn file_attribute_from_fat(attributes: u8) -> FileAttributes {
    let mut file_attributes = FileAttributes::EMPTY;
    if attributes & attrs::READ_ONLY == attrs::READ_ONLY {
        file_attributes |= FileAttributes::READ_ONLY;
    }
    if attributes & attrs::HIDDEN == attrs::HIDDEN {
        file_attributes |= FileAttributes::HIDDEN;
    }
    if attributes & attrs::SYSTEM == attrs::SYSTEM {
        file_attributes |= FileAttributes::SYSTEM;
    }
    if attributes & attrs::VOLUME_ID == attrs::VOLUME_ID {
        file_attributes |= FileAttributes::VOLUME_LABEL;
    }
    if attributes & attrs::DIRECTORY == attrs::DIRECTORY {
        file_attributes |= FileAttributes::DIRECTORY;
    }
    if attributes & attrs::ARCHIVE == attrs::ARCHIVE {
        file_attributes |= FileAttributes::ARCHIVE;
    }
    file_attributes
}

fn file_attribute_to_fat(attributes: FileAttributes) -> u8 {
    let mut fat_attributes = 0;
    if attributes.contains(FileAttributes::READ_ONLY) {
        fat_attributes |= attrs::READ_ONLY;
    }
    if attributes.contains(FileAttributes::HIDDEN) {
        fat_attributes |= attrs::HIDDEN;
    }
    if attributes.contains(FileAttributes::SYSTEM) {
        fat_attributes |= attrs::SYSTEM;
    }
    if attributes.contains(FileAttributes::VOLUME_LABEL) {
        fat_attributes |= attrs::VOLUME_ID;
    }
    if attributes.contains(FileAttributes::DIRECTORY) {
        fat_attributes |= attrs::DIRECTORY;
    }
    if attributes.contains(FileAttributes::ARCHIVE) {
        fat_attributes |= attrs::ARCHIVE;
    }
    fat_attributes
}

fn long_entries_name_merge(entries: impl DoubleEndedIterator<Item = String>) -> String {
    let mut name = String::new();
    entries.rev().for_each(|s| name.push_str(&s));
    name
}

fn create_dir_entries(
    name: &str,
    attributes: FileAttributes,
) -> (DirectoryEntryNormal, Vec<DirectoryEntryLong>) {
    // create short name entry
    let mut short_name = [0; 11];

    let (mut filename, extension) = match name.find('.') {
        Some(i) => {
            let (filename, extension) = name.split_at(i);
            (filename, &extension[1..])
        }
        None => (name, ""),
    };

    let mut more_than_8 = false;

    if filename.len() > 8 {
        filename = &filename[..6];
        more_than_8 = true;
    } else {
        let len = filename.len().min(8);
        filename = &filename[..len];
    }
    assert!(filename.len() <= 8);

    for (i, c) in short_name.iter_mut().enumerate().take(8) {
        *c = if i < filename.len() {
            filename.as_bytes()[i].to_ascii_uppercase()
        } else {
            b' '
        };
    }
    if more_than_8 {
        short_name[6] = b'~';
        short_name[7] = b'1';
    }

    for i in 0..3 {
        short_name[8 + i] = if i < extension.len() {
            extension.as_bytes()[i].to_ascii_uppercase()
        } else {
            b' '
        };
    }

    // TODO: add support for time and date
    let normal_entry = DirectoryEntryNormal {
        short_name,
        attributes: file_attribute_to_fat(attributes),
        _nt_reserved: 0,
        creation_time_tenths_of_seconds: 0,
        creation_time: 0,
        creation_date: 0,
        last_access_date: 0,
        first_cluster_hi: 0,
        last_modification_time: 0,
        last_modification_date: 0,
        first_cluster_lo: 0,
        file_size: 0,
    };

    let short_name_checksum = normal_entry.name_checksum();

    // create long name entries
    let mut long_name_entries = Vec::new();
    let mut sequence_number = 1;
    let mut long_name = name;
    loop {
        if long_name.is_empty() {
            break;
        }

        let len = long_name.len().min(13);
        let mut name_part = long_name[..len].chars();

        long_name = &long_name[len..];

        let mut name1 = [0; 5];
        let mut name2 = [0; 6];
        let mut name3 = [0; 2];

        for c in &mut name1 {
            *c = name_part.next().unwrap_or('\0') as u16;
        }
        for c in &mut name2 {
            *c = name_part.next().unwrap_or('\0') as u16;
        }
        for c in &mut name3 {
            *c = name_part.next().unwrap_or('\0') as u16;
        }

        let mut entry = DirectoryEntryLong {
            sequence_number,
            name1,
            attributes: attrs::LONG_NAME,
            long_name_type: 0,
            checksum: short_name_checksum,
            name2,
            _zero: 0,
            name3,
        };

        sequence_number += 1;

        // mark the last entry
        if long_name.is_empty() {
            entry.sequence_number |= 0x40;
        }

        long_name_entries.push(entry);
    }

    (normal_entry, long_name_entries)
}

fn increment_short_name(short_name: &mut [u8; 11]) {
    let base_name = &mut short_name[..8];

    let mut telda_pos = base_name
        .iter()
        .position(|c| *c == b'~')
        .expect("Telda position be present");

    assert!(telda_pos <= 6);
    let current_num_size = 8 - telda_pos - 1;
    let current_num = base_name[telda_pos + 1..].iter().fold(0u32, |acc, x| {
        assert!(*x >= b'0' && *x <= b'9');
        acc * 10 + (x - b'0') as u32
    });

    let new_num = current_num + 1;
    if new_num > 999999 {
        panic!("Short name exceeded limit 999999");
    }

    let new_num_str = format!("{}", new_num);
    if new_num_str.len() > current_num_size {
        telda_pos -= 1;
    }

    assert_eq!(base_name[telda_pos + 1..].len(), new_num_str.len());
    base_name[telda_pos + 1..].copy_from_slice(new_num_str.as_bytes());
}

#[derive(Debug)]
pub enum FatError {
    InvalidBootSector,
    UnexpectedFatEntry,
    NotEnoughSpace,
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

    // SAFETY: This is a valid, allocated memory
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

    pub fn to_u32(self, ty: FatType) -> Option<u32> {
        match self {
            FatEntry::Free => Some(0),
            FatEntry::EndOfChain => match ty {
                FatType::Fat12 => Some(0xFF8),
                FatType::Fat16 => Some(0xFFF8),
                FatType::Fat32 => Some(0x0FFF_FFF8),
            },
            FatEntry::Bad => match ty {
                FatType::Fat12 => Some(0xFF7),
                FatType::Fat16 => Some(0xFFF7),
                FatType::Fat32 => Some(0x0FFF_FFF7),
            },
            FatEntry::Next(entry) => match (ty, entry) {
                (FatType::Fat12, 0x002..=0xFF6) => Some(entry),
                (FatType::Fat16, 0x002..=0xFFF6) => Some(entry),
                (FatType::Fat32, 0x002..=0x0FFF_FFF6) => Some(entry),
                _ => None,
            },
            FatEntry::Reserved => None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryEntryState {
    Free,
    FreeAndLast,
    Used,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, packed)]
struct DirectoryEntryNormal {
    short_name: [u8; 11],
    attributes: u8,
    _nt_reserved: u8,
    creation_time_tenths_of_seconds: u8,
    creation_time: u16,
    creation_date: u16,
    last_access_date: u16,
    first_cluster_hi: u16,
    last_modification_time: u16,
    last_modification_date: u16,
    first_cluster_lo: u16,
    file_size: u32,
}

impl DirectoryEntryNormal {
    pub fn name(&self) -> String {
        let base_name = &self.short_name[..8];
        let base_name_end = 8 - base_name.iter().rev().position(|&c| c != 0x20).unwrap();
        let extension = &self.short_name[8..11];

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
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_hi as u32) << 16) | self.first_cluster_lo as u32
    }

    pub fn name_checksum(&self) -> u8 {
        let mut checksum = 0u8;
        for &c in self.short_name.iter() {
            checksum = ((checksum & 1) << 7)
                .wrapping_add(checksum >> 1)
                .wrapping_add(c);
        }
        checksum
    }
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
struct DirectoryEntryLong {
    sequence_number: u8,
    name1: [u16; 5],
    attributes: u8,
    long_name_type: u8,
    checksum: u8,
    name2: [u16; 6],
    _zero: u16,
    name3: [u16; 2],
}

impl DirectoryEntryLong {
    pub fn name(&self) -> String {
        let name1 = unsafe { &core::ptr::addr_of!(self.name1).read_unaligned() };
        let name2 = unsafe { &core::ptr::addr_of!(self.name2).read_unaligned() };
        let name3 = unsafe { &core::ptr::addr_of!(self.name3).read_unaligned() };

        // construct an iterator and fill the string part
        let name_iter = name1
            .iter()
            .chain(name2)
            .chain(name3)
            .cloned()
            .take_while(|c| c != &0);

        let mut name_part = String::with_capacity(13);
        char::decode_utf16(name_iter)
            .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
            .for_each(|c| name_part.push(c));

        name_part
    }
}

enum DirectoryEntry<'a> {
    Normal(&'a mut DirectoryEntryNormal),
    Long(&'a mut DirectoryEntryLong),
}

impl<'a> DirectoryEntry<'a> {
    pub fn from_raw(raw: &mut [u8]) -> DirectoryEntry {
        assert_eq!(raw.len(), DIRECTORY_ENTRY_SIZE as usize);
        let normal = unsafe {
            raw.as_mut_ptr()
                .cast::<DirectoryEntryNormal>()
                .as_mut()
                .unwrap()
        };
        let attributes = normal.attributes;
        if attributes & attrs::LONG_NAME == attrs::LONG_NAME {
            DirectoryEntry::Long(unsafe {
                raw.as_mut_ptr()
                    .cast::<DirectoryEntryLong>()
                    .as_mut()
                    .unwrap()
            })
        } else {
            DirectoryEntry::Normal(normal)
        }
    }

    pub fn state(&self) -> DirectoryEntryState {
        let first_byte = match self {
            DirectoryEntry::Normal(entry) => entry.short_name[0],
            DirectoryEntry::Long(entry) => entry.sequence_number,
        };
        match first_byte {
            0x00 => DirectoryEntryState::FreeAndLast,
            0xE5 => DirectoryEntryState::Free,
            _ => DirectoryEntryState::Used,
        }
    }

    pub fn as_normal(&self) -> &DirectoryEntryNormal {
        match self {
            DirectoryEntry::Normal(entry) => entry,
            _ => panic!("expected normal entry"),
        }
    }

    pub fn as_normal_mut(&mut self) -> &mut DirectoryEntryNormal {
        match self {
            DirectoryEntry::Normal(entry) => entry,
            _ => panic!("expected normal entry"),
        }
    }

    pub fn is_long(&self) -> bool {
        matches!(self, DirectoryEntry::Long(_))
    }

    pub fn as_long(&self) -> &DirectoryEntryLong {
        match self {
            DirectoryEntry::Long(entry) => entry,
            _ => panic!("expected long entry"),
        }
    }

    fn write_long(&mut self, new_entry: DirectoryEntryLong) {
        assert_eq!(new_entry.attributes & attrs::LONG_NAME, attrs::LONG_NAME);
        match self {
            DirectoryEntry::Long(entry) => {
                **entry = new_entry;
            }
            DirectoryEntry::Normal(entry) => {
                // convert to long entry
                // Safety: we know that these share the same memory layout
                unsafe {
                    let long_entry = core::ptr::from_mut(*entry)
                        .cast::<DirectoryEntryLong>()
                        .as_mut()
                        .unwrap();
                    *long_entry = new_entry;
                    *self = DirectoryEntry::Long(long_entry);
                }
            }
        }
    }

    fn write_normal(&mut self, new_entry: DirectoryEntryNormal) {
        assert_ne!(new_entry.attributes & attrs::LONG_NAME, attrs::LONG_NAME);
        match self {
            DirectoryEntry::Normal(entry) => {
                **entry = new_entry;
            }
            DirectoryEntry::Long(entry) => {
                // convert to normal entry
                // Safety: we know that these share the same memory layout
                unsafe {
                    let normal_entry = core::ptr::from_mut(entry)
                        .cast::<DirectoryEntryNormal>()
                        .as_mut()
                        .unwrap();
                    *normal_entry = new_entry;
                    *self = DirectoryEntry::Normal(normal_entry);
                }
            }
        }
    }
}

/// A custom version of `fs::Node` for fat systems
pub struct FatNode {
    normal_entry: DirectoryEntryNormal,
    long_name: Option<String>,

    parent_dir_sector: u64,
    parent_dir_index: u16,
}

impl FatNode {
    pub fn matches(&self, matcher: &str) -> bool {
        // First, check if we have a long name and if it matches
        if let Some(long_name) = &self.long_name {
            if long_name.eq_ignore_ascii_case(matcher) {
                return true;
            }
        }

        // If no long name match, check the short name
        let short_name = self.normal_entry.name();
        short_name.eq_ignore_ascii_case(matcher)
    }
}

impl From<FatNode> for Node {
    fn from(value: FatNode) -> Self {
        Node::new(
            value.long_name.unwrap_or(value.normal_entry.name()),
            file_attribute_from_fat(value.normal_entry.attributes),
            value.normal_entry.first_cluster().into(),
            value.normal_entry.file_size.into(),
            value.parent_dir_sector,
            value.parent_dir_index,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct DirectoryIterSavedPosition {
    cluster: u32,
    sector: u32,
    entry: u16,
}

#[derive(Debug, Clone)]
enum Directory {
    RootFat12_16 {
        start_sector: u32,
        size_in_sectors: u32,
    },
    Normal {
        inode: DirectoryNode,
    },
}

pub struct DirectoryIterator<'a> {
    dir: Directory,
    filesystem: &'a FatFilesystem,
    // only hold one sector
    current_sector: Vec<u8>,
    current_sector_index: u32,
    current_cluster: u32,
    current_sector_dirty: bool,
    entry_index_in_sector: u16,
}

impl DirectoryIterator<'_> {
    fn new(
        filesystem: &FatFilesystem,
        dir: Directory,
    ) -> Result<DirectoryIterator, FileSystemError> {
        let (sector_index, current_cluster, current_sector) = match dir {
            Directory::RootFat12_16 { start_sector, .. } => (
                start_sector,
                0,
                filesystem.read_sectors_no_cache(start_sector, 1)?,
            ),
            Directory::Normal { ref inode } => {
                if matches!(filesystem.fat_type(), FatType::Fat12 | FatType::Fat16)
                    && inode.start_cluster() == 0
                {
                    // looks like we got back using `..` to the root, thus, we should use the root directly
                    return Self::new(filesystem, filesystem.open_root_dir()?);
                }

                let start_sector = filesystem.first_sector_of_cluster(inode.start_cluster() as u32);

                (
                    start_sector,
                    inode.start_cluster() as u32,
                    filesystem.read_sectors_no_cache(start_sector, 1)?,
                )
            }
        };
        Ok(DirectoryIterator {
            dir,
            filesystem,
            current_sector,
            current_cluster,
            current_sector_dirty: false,
            current_sector_index: sector_index,
            entry_index_in_sector: 0,
        })
    }

    // return true if we got more sectors and we can continue
    fn next_sector(&mut self) -> Result<bool, FileSystemError> {
        self.flush_current_sector();

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
                    let next_cluster = self.filesystem.fat.next_cluster(self.current_cluster);
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

        self.current_sector = self
            .filesystem
            .read_sectors_no_cache(next_sector_index, 1)?;
        self.current_sector_index = next_sector_index;
        self.entry_index_in_sector = 0;
        Ok(true)
    }

    fn get_next_entry(&mut self) -> Result<DirectoryEntry, FileSystemError> {
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
        let entry = &mut self.current_sector[entry_start..entry_end];
        self.entry_index_in_sector += 1;

        assert_eq!(entry.len(), DIRECTORY_ENTRY_SIZE as usize);
        Ok(DirectoryEntry::from_raw(entry))
    }

    fn mark_sector_dirty(&mut self) {
        self.current_sector_dirty = true;
    }

    fn flush_current_sector(&mut self) {
        if self.current_sector_dirty {
            let start_sector = self.current_sector_index;
            self.filesystem
                .write_sectors(start_sector, &self.current_sector)
                .unwrap();
            self.current_sector_dirty = false;
        }
    }

    fn restore_at(&mut self, saved_pos: DirectoryIterSavedPosition) -> Result<(), FileSystemError> {
        self.current_cluster = saved_pos.cluster;
        self.entry_index_in_sector = saved_pos.entry;
        if self.current_sector_index != saved_pos.sector {
            self.flush_current_sector();

            // we need to read the current sector
            self.current_sector_index = saved_pos.sector;
            self.current_sector = self
                .filesystem
                .read_sectors_no_cache(self.current_sector_index, 1)?;
        }
        Ok(())
    }

    fn save_current(&self) -> DirectoryIterSavedPosition {
        DirectoryIterSavedPosition {
            cluster: self.current_cluster,
            sector: self.current_sector_index,
            // if this is the first entry, keep it zero, otherwise it will always be at least 1
            entry: self.entry_index_in_sector.saturating_sub(1),
        }
    }

    fn add_entry(
        &mut self,
        mut entry: DirectoryEntryNormal,
        long_entries: Vec<DirectoryEntryLong>,
    ) -> Result<Node, FileSystemError> {
        // used to check if the entry is already in the directory
        let mut new_entry_short_name = entry.short_name;
        let new_entry_long_name =
            long_entries_name_merge(long_entries.iter().map(DirectoryEntryLong::name));

        let needed_entries = long_entries.len() + 1;

        let mut is_last = false;

        let mut first_free = None;
        let mut running_free = 0;
        let mut current_long_entries_name = Vec::new();
        let long_entries_require_free = Cell::new(false);

        let mut is_already_exists = |entry: DirectoryEntry| -> bool {
            if entry.is_long() {
                if long_entries_require_free.get() {
                    long_entries_require_free.set(false);
                    current_long_entries_name.clear();
                }
                current_long_entries_name.push(entry.as_long().name());
            } else {
                let long_name = long_entries_name_merge(current_long_entries_name.drain(..));

                if long_name.eq_ignore_ascii_case(&new_entry_long_name) {
                    return true;
                }
                // long name doesn't match, but short one matches, meaning the short name got clipped
                if entry.as_normal().short_name == new_entry_short_name {
                    increment_short_name(&mut new_entry_short_name);
                }
            }

            false
        };

        loop {
            let entry = self.get_next_entry()?;
            match entry.state() {
                DirectoryEntryState::FreeAndLast => {
                    long_entries_require_free.set(true);

                    is_last = true;
                    // this is the first and last entry
                    if first_free.is_none() {
                        first_free = Some(self.save_current());
                    }
                    break;
                }
                DirectoryEntryState::Free => {
                    long_entries_require_free.set(true);

                    // make sure we have enough free entries
                    if first_free.is_none() {
                        first_free = Some(self.save_current());
                    }
                    running_free += 1;
                    if running_free == needed_entries {
                        break;
                    }
                }
                DirectoryEntryState::Used => {
                    // reset the running free
                    running_free = 0;
                    first_free = None;
                    if is_already_exists(entry) {
                        return Err(FileSystemError::AlreadyExists);
                    }
                }
            }
        }

        if !is_last {
            // keep looking through all Used
            loop {
                let entry = self.get_next_entry()?;
                match entry.state() {
                    DirectoryEntryState::FreeAndLast => break,
                    DirectoryEntryState::Used => {
                        if is_already_exists(entry) {
                            return Err(FileSystemError::AlreadyExists);
                        }
                    }
                    _ => {}
                }
            }
        }

        // update the short_name if it changed/incremented
        entry.short_name = new_entry_short_name;

        assert!(first_free.is_some());
        let first_free = first_free.unwrap();

        self.restore_at(first_free)?;

        // write the long entries
        let mut current_entry = self.get_next_entry()?;

        for long_entry in long_entries.into_iter().rev() {
            current_entry.write_long(long_entry);
            self.mark_sector_dirty();
            current_entry = self.get_next_entry()?;
        }

        // write the normal entry
        current_entry.write_normal(entry);
        self.mark_sector_dirty();

        // go back
        self.restore_at(first_free)?;
        let node = self.next();

        if is_last {
            let pos = self.save_current();
            // that was the last entry, make sure the new last is valid
            current_entry = self.get_next_entry()?;
            current_entry.as_normal_mut().short_name[0] = 0x00;
            self.mark_sector_dirty();
            // restore, so that next calls to `add_entry` can continue without missing an entry
            self.restore_at(pos)?;
        }

        Ok(node.expect("node should be created").into())
    }
}

impl Iterator for DirectoryIterator<'_> {
    type Item = FatNode;

    fn next(&mut self) -> Option<Self::Item> {
        let mut entry = self.get_next_entry().ok()?;

        loop {
            match entry.state() {
                DirectoryEntryState::FreeAndLast => {
                    return None;
                }
                DirectoryEntryState::Free => {
                    entry = self.get_next_entry().ok()?;
                }
                _ => break,
            }
        }

        let long_name = if entry.is_long() {
            let mut long_entry = entry.as_long().clone();
            // long file name
            // this should be the last
            assert_eq!(long_entry.sequence_number & 0x40, 0x40);
            let number_of_entries = long_entry.sequence_number & 0x3F;
            let mut long_name_entries = Vec::with_capacity(number_of_entries as usize);
            // skip all long file name entries
            for i in 0..number_of_entries {
                let name_part = long_entry.name();

                // add to the entries
                long_name_entries.push(name_part);

                // next entry
                entry = self.get_next_entry().ok()?;
                if i + 1 < number_of_entries {
                    long_entry = entry.as_long().clone();
                }
            }

            Some(long_entries_name_merge(long_name_entries.into_iter()))
        } else {
            None
        };

        let normal_entry = entry.as_normal().clone();
        assert!(self.entry_index_in_sector > 0);

        Some(FatNode {
            normal_entry,
            long_name,
            parent_dir_sector: self.current_sector_index.into(),
            parent_dir_index: self.entry_index_in_sector - 1,
        })
    }
}

impl Drop for DirectoryIterator<'_> {
    fn drop(&mut self) {
        self.flush_current_sector();
    }
}

#[derive(Debug)]
struct ClusterCacheEntry {
    #[allow(dead_code)]
    cluster: u32,
    /// Number of active users of this cluster
    reference_count: u32,
    dirty_range: Option<Range<usize>>,
    data: NoDebug<Vec<u8>>,
}

#[derive(Default, Debug)]
struct ClusterCache {
    entries: BTreeMap<u32, ClusterCacheEntry>,
}

impl ClusterCache {
    pub fn try_get_cluster_mut(&mut self, cluster: u32) -> Option<&mut ClusterCacheEntry> {
        self.entries.get_mut(&cluster)
    }

    pub fn try_get_cluster_locked(&mut self, cluster: u32) -> Option<&mut ClusterCacheEntry> {
        if let Some(entry) = self.entries.get_mut(&cluster) {
            entry.reference_count += 1;
            return Some(entry);
        }
        None
    }

    pub fn insert_cluster(&mut self, cluster: u32, data: Vec<u8>) -> &mut ClusterCacheEntry {
        let entry = ClusterCacheEntry {
            cluster,
            reference_count: 1,
            data: NoDebug(data),
            dirty_range: None,
        };
        self.entries.entry(cluster).or_insert(entry)
    }

    pub fn release_cluster(&mut self, cluster: u32) -> Option<ClusterCacheEntry> {
        match self.entries.entry(cluster) {
            alloc::collections::btree_map::Entry::Vacant(_) => None,
            alloc::collections::btree_map::Entry::Occupied(mut entry) => {
                let cluster = entry.get_mut();
                cluster.reference_count -= 1;
                if cluster.reference_count == 0 {
                    return Some(entry.remove());
                }
                None
            }
        }
    }

    pub fn release_all(&mut self) -> BTreeMap<u32, ClusterCacheEntry> {
        core::mem::take(&mut self.entries)
    }
}

/// Buffer for reading or writing file data
enum FileAccessBuffer<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl FileAccessBuffer<'_> {
    pub fn len(&self) -> usize {
        match self {
            FileAccessBuffer::Read(data) => data.len(),
            FileAccessBuffer::Write(data) => data.len(),
        }
    }
}

/// File Allocation Table buffer
#[derive(Debug)]
struct Fat {
    buffer: NoDebug<Vec<u8>>,
    sector_size: u16,
    fat_type: FatType,
    dirty: bool,
    /// One bit for each sector in the FAT
    dirty_bitmap: Vec<u64>,
}

impl Fat {
    /// A temporary initializer for the FAT, will be replaced with [`Fat::load`]
    fn new() -> Self {
        Self {
            buffer: NoDebug(Vec::new()),
            sector_size: 0,
            fat_type: FatType::Fat12,
            dirty: false,
            dirty_bitmap: Vec::new(),
        }
    }

    fn load(filesystem: &FatFilesystem) -> Result<Self, FileSystemError> {
        let fats_size_in_sectors = filesystem.boot_sector.fat_size_in_sectors()
            * filesystem.boot_sector.number_of_fats() as u32;
        let fat_start_sector = filesystem.boot_sector.fat_start_sector();

        let buffer = filesystem.read_sectors_no_cache(fat_start_sector, fats_size_in_sectors)?;
        let fat_type = filesystem.fat_type();

        Ok(Self {
            buffer: NoDebug(buffer),
            sector_size: filesystem.boot_sector.bytes_per_sector(),
            fat_type,
            dirty: false,
            dirty_bitmap: vec![0; (fats_size_in_sectors as usize + 63) / 64],
        })
    }

    // return an iterator of (sector_index, sector_data) for all dirty sectors
    fn dirty_sectors(&self) -> Option<impl Iterator<Item = (u32, &[u8])>> {
        if !self.dirty {
            return None;
        }

        Some(
            self.dirty_bitmap
                .iter()
                .enumerate()
                .filter_map(move |(i, dirty)| {
                    if *dirty == 0 {
                        return None;
                    }

                    let mut dirty = *dirty;
                    let mut bit = 0;

                    Some(core::iter::from_fn(move || {
                        while dirty != 0 {
                            if dirty & 1 != 0 {
                                let sector = (i * 64 + bit) as u32;
                                let sector_start = sector * self.sector_size as u32;
                                let sector_end = sector_start + self.sector_size as u32;
                                let data = &self.buffer[sector_start as usize..sector_end as usize];
                                dirty >>= 1;
                                bit += 1;
                                return Some((sector, data));
                            }
                            dirty >>= 1;
                            bit += 1;
                        }
                        None
                    }))
                })
                .flatten(),
        )
    }

    fn clear_dirty(&mut self) {
        if self.dirty {
            self.dirty = false;
            self.dirty_bitmap.iter_mut().for_each(|d| *d = 0);
        }
    }

    fn read_fat_entry(&self, entry: u32) -> FatEntry {
        let fat_offset = match self.fat_type {
            FatType::Fat12 => entry * 3 / 2,
            FatType::Fat16 => entry * 2,
            FatType::Fat32 => entry * 4,
        } as usize;
        assert!(fat_offset < self.buffer.0.len(), "FAT entry out of bounds");
        let ptr = unsafe { self.buffer.0.as_ptr().add(fat_offset) };

        let entry = match self.fat_type {
            FatType::Fat12 => {
                let byte1 = self.buffer.0[fat_offset];
                let byte2 = self.buffer.0[fat_offset + 1];
                if entry & 1 == 1 {
                    ((byte2 as u32) << 4) | ((byte1 as u32) >> 4)
                } else {
                    (((byte2 as u32) & 0xF) << 8) | (byte1 as u32)
                }
            }
            FatType::Fat16 => unsafe { (*(ptr as *const u16)) as u32 },
            FatType::Fat32 => unsafe { (*(ptr as *const u32)) & 0x0FFF_FFFF },
        };

        FatEntry::from_u32(self.fat_type, entry)
    }

    fn mark_sector_dirty(&mut self, sector: usize) {
        let index = sector / 64;
        let bit = sector % 64;
        self.dirty_bitmap[index] |= 1 << bit;
    }

    fn write_fat_entry(&mut self, entry: u32, fat_entry: FatEntry) {
        let fat_offset = match self.fat_type {
            FatType::Fat12 => entry * 3 / 2,
            FatType::Fat16 => entry * 2,
            FatType::Fat32 => entry * 4,
        } as usize;
        assert!(fat_offset < self.buffer.0.len(), "FAT entry out of bounds");
        let ptr = unsafe { self.buffer.0.as_mut_ptr().add(fat_offset) };

        let new_entry = fat_entry.to_u32(self.fat_type).expect("invalid FAT entry");

        match self.fat_type {
            FatType::Fat12 => {
                if entry & 1 == 1 {
                    self.buffer.0[fat_offset] =
                        (self.buffer.0[fat_offset] & 0x0F) | (new_entry << 4) as u8;
                    self.buffer.0[fat_offset + 1] = (new_entry >> 4) as u8;
                } else {
                    self.buffer.0[fat_offset] = new_entry as u8;
                    self.buffer.0[fat_offset + 1] =
                        (self.buffer.0[fat_offset + 1] & 0xF0) | ((new_entry >> 8) as u8);
                }
            }
            FatType::Fat16 => unsafe { *(ptr as *mut u16) = new_entry as u16 },
            FatType::Fat32 => unsafe { *(ptr as *mut u32) = new_entry },
        }

        self.mark_sector_dirty(fat_offset / self.sector_size as usize);

        // can cross sector boundary sometimes
        if self.fat_type == FatType::Fat12 {
            self.mark_sector_dirty((fat_offset + 1) / self.sector_size as usize);
        }

        self.dirty = true;
    }

    fn find_free_cluster(&self) -> Option<u32> {
        let fat_size = self.buffer.0.len();

        let number_of_fat_entries = match self.fat_type {
            FatType::Fat12 => fat_size * 2 / 3,
            FatType::Fat16 => fat_size / 2,
            FatType::Fat32 => fat_size / 4,
        } as u32;

        (2..number_of_fat_entries).find(|&i| self.read_fat_entry(i) == FatEntry::Free)
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
}

#[derive(Debug)]
pub struct FatFilesystem {
    start_lba: u32,
    #[allow(dead_code)]
    size_in_sectors: u32,
    boot_sector: Box<FatBootSector>,
    fat: Fat,
    device: NoDebug<Arc<IdeDevice>>,
    cluster_cache: ClusterCache,
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
            fat: Fat::new(),
            device: NoDebug(device),
            cluster_cache: ClusterCache::default(),
        };

        // TODO: replace by lazily reading FAT when needed
        s.fat = Fat::load(&s)?;

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

    fn read_sectors_no_cache(
        &self,
        start_sector: u32,
        count: u32,
    ) -> Result<Vec<u8>, FileSystemError> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let sector_size = self.boot_sector.bytes_per_sector() as usize;
        let mut sectors = vec![0; sector_size * count as usize];

        let start_lba = (self.start_lba + start_sector) as u64;
        self.device
            .read_sync(start_lba, &mut sectors)
            .map_err(|e| FileSystemError::DiskReadError {
                sector: start_lba,
                error: e,
            })?;

        Ok(sectors)
    }

    fn write_sectors(&self, start_sector: u32, data: &[u8]) -> Result<(), FileSystemError> {
        if data.is_empty() {
            return Ok(());
        }
        assert_eq!(data.len() % self.boot_sector.bytes_per_sector() as usize, 0);
        let start_lba = (self.start_lba + start_sector) as u64;
        self.device
            .write_sync(start_lba, data)
            .map_err(|e| FileSystemError::DiskReadError {
                sector: start_lba,
                error: e,
            })?;
        Ok(())
    }

    fn get_cluster(&mut self, cluster: u32) -> Option<&mut ClusterCacheEntry> {
        self.cluster_cache.try_get_cluster_mut(cluster)
    }

    fn lock_cluster(&mut self, cluster: u32) -> Result<&mut ClusterCacheEntry, FileSystemError> {
        // TODO: fix this borrow checker issue when the language fixes it
        //       without borrow checker error we won't need to call `try_get_cluster_locked` twice
        if self.cluster_cache.try_get_cluster_mut(cluster).is_none() {
            let data = self.read_sectors_no_cache(
                self.first_sector_of_cluster(cluster),
                self.boot_sector.sectors_per_cluster().into(),
            )?;
            return Ok(self.cluster_cache.insert_cluster(cluster, data));
        }

        Ok(self.cluster_cache.try_get_cluster_locked(cluster).unwrap())
    }

    /// Helper method to write the dirty parts of a cluster into disk
    fn flush_cluster_dirty_range_file(
        &mut self,
        inode: &FileNode,
        cluster_data: &[u8],
        cluster_num: u32,
        dirty_range: Range<usize>,
    ) -> Result<(), FileSystemError> {
        self.flush_fat()?;
        self.update_directory_entry(inode, |entry| {
            entry.file_size = inode.size() as u32;
        })?;

        self.flush_cluster_dirty_range(cluster_data, cluster_num, dirty_range)
    }

    fn flush_cluster_dirty_range(
        &mut self,
        cluster_data: &[u8],
        cluster_num: u32,
        dirty_range: Range<usize>,
    ) -> Result<(), FileSystemError> {
        let start_byte_offset = align_down(
            dirty_range.start,
            self.boot_sector.bytes_per_sector() as usize,
        );
        let end_byte_offset = align_up(
            dirty_range.end,
            self.boot_sector.bytes_per_sector() as usize,
        );
        assert!(
            start_byte_offset < self.boot_sector.bytes_per_cluster() as usize,
            "start_byte_offset: {start_byte_offset} < {}",
            self.boot_sector.bytes_per_cluster()
        );
        assert!(
            end_byte_offset <= self.boot_sector.bytes_per_cluster() as usize,
            "end_byte_offset: {end_byte_offset} < {}",
            self.boot_sector.bytes_per_cluster()
        );
        assert!(
            start_byte_offset < end_byte_offset,
            "start_byte_offset, end_byte_offset {start_byte_offset} < {end_byte_offset}"
        );

        // write back
        let start_sector = self.first_sector_of_cluster(cluster_num)
            + (start_byte_offset as u32 / self.boot_sector.bytes_per_sector() as u32);
        self.write_sectors(
            start_sector,
            &cluster_data[start_byte_offset..end_byte_offset],
        )?;

        Ok(())
    }

    fn release_cluster(&mut self, inode: &FileNode, cluster: u32) -> Result<(), FileSystemError> {
        if let Some(cluster) = self.cluster_cache.release_cluster(cluster) {
            if let Some(dirty_range) = cluster.dirty_range {
                self.flush_cluster_dirty_range_file(
                    inode,
                    &cluster.data,
                    cluster.cluster,
                    dirty_range,
                )?;
            }
        }
        Ok(())
    }

    /// Same as `release_cluster`, but doesn't release it, i.e. the cluster will
    /// still be used, but the `dirty` flag is removed
    fn flush_cluster(&mut self, inode: &FileNode, cluster: u32) -> Result<(), FileSystemError> {
        let mut cluster_data: Option<NoDebug<Vec<u8>>> = None;

        if let Some(cluster) = self.cluster_cache.try_get_cluster_mut(cluster) {
            if let Some(dirty_range) = cluster.dirty_range.take() {
                let cluster_num = cluster.cluster;
                cluster_data = Some(NoDebug(Vec::new()));
                core::mem::swap(&mut cluster.data, cluster_data.as_mut().unwrap());
                self.flush_cluster_dirty_range_file(
                    inode,
                    cluster_data.as_ref().unwrap(),
                    cluster_num,
                    dirty_range,
                )?;
            }
        }
        // swap back
        // This is annoying, and its only used to get around the borrow checker
        // probably there is a way to better achieve this
        // FIXME: find better solution
        if let Some(cluster) = self.cluster_cache.try_get_cluster_mut(cluster) {
            if let Some(cluster_data) = cluster_data.as_mut() {
                core::mem::swap(&mut cluster.data, cluster_data);
            }
        }

        Ok(())
    }

    fn flush_fat(&mut self) -> Result<(), FileSystemError> {
        if let Some(dirt_iter) = self.fat.dirty_sectors() {
            for (sector, data) in dirt_iter {
                self.write_sectors(self.boot_sector.fat_start_sector() + sector, data)?;
            }
        }
        self.fat.clear_dirty();

        Ok(())
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
                let inode = DirectoryNode::without_parent(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    root_cluster as u64,
                );
                Ok(Directory::Normal { inode })
            }
        }
    }

    fn open_root_dir_inode(&self) -> Result<DirectoryNode, FileSystemError> {
        match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // use a special inode for root
                let inode = DirectoryNode::without_parent(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    0,
                );

                Ok(inode)
            }
            FatType::Fat32 => {
                let root_cluster =
                    unsafe { self.boot_sector.boot_sector.extended.fat32.root_cluster };
                let inode = DirectoryNode::without_parent(
                    String::from("/"),
                    file_attribute_from_fat(attrs::DIRECTORY),
                    root_cluster as u64,
                );
                Ok(inode)
            }
        }
    }

    pub fn open_dir_inode(
        &self,
        inode: &DirectoryNode,
    ) -> Result<DirectoryIterator, FileSystemError> {
        let dir = match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                // try to see if this is the root
                // this could be 0 if we are back from using `..` to the root
                if inode.start_cluster() == 0 {
                    self.open_root_dir()?
                } else {
                    Directory::Normal {
                        inode: inode.clone(),
                    }
                }
            }
            FatType::Fat32 => Directory::Normal {
                inode: inode.clone(),
            },
        };

        DirectoryIterator::new(self, dir)
    }

    fn read_write_file(
        &mut self,
        inode: &FileNode,
        position: u32,
        mut buf: FileAccessBuffer,
        access_helper: &mut AccessHelper,
    ) -> Result<u64, FileSystemError> {
        if position >= inode.size() as u32 {
            return Ok(0);
        }
        let remaining_file = inode.size() as u32 - position;
        let max_to_access = (buf.len() as u32).min(remaining_file);
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster();
        let mut position_in_cluster = position % bytes_per_cluster;
        let cluster_index = position / bytes_per_cluster;

        // seek happened or switch exactly to next cluster after last call
        if access_helper.cluster_index != cluster_index as u64 {
            if access_helper.current_cluster != 0 {
                self.release_cluster(inode, access_helper.current_cluster as u32)?;
            }
            access_helper.current_cluster = 0;
        }

        // starting out
        let mut cluster_entry = if access_helper.current_cluster == 0 {
            let mut cluster = inode.start_cluster() as u32;

            // cannot be empty, or be the root
            assert_ne!(cluster, 0);

            for _ in 0..cluster_index {
                cluster = self
                    .fat
                    .next_cluster(cluster)?
                    .ok_or(FatError::UnexpectedFatEntry)?;
            }

            access_helper.current_cluster = cluster as u64;
            access_helper.cluster_index = cluster_index as u64;
            self.lock_cluster(access_helper.current_cluster as u32)?
        } else {
            self.get_cluster(access_helper.current_cluster as u32)
                .expect("This should be cached")
        };
        let mut cluster = access_helper.current_cluster as u32;

        // read/write
        let mut accessed = 0;
        while accessed < max_to_access as usize {
            let remaining_in_cluster = &mut cluster_entry.data[position_in_cluster as usize..];

            let to_access = core::cmp::min(
                remaining_in_cluster.len() as u32,
                max_to_access - accessed as u32,
            ) as usize;
            match buf {
                FileAccessBuffer::Read(ref mut buf) => {
                    buf[accessed..accessed + to_access]
                        .copy_from_slice(&remaining_in_cluster[..to_access]);
                }
                FileAccessBuffer::Write(buf) => {
                    let range_start = position_in_cluster as usize;
                    let range_end = range_start + to_access;

                    if let Some(dirty_range) = cluster_entry.dirty_range.as_mut() {
                        dirty_range.start = dirty_range.start.min(range_start);
                        dirty_range.end = dirty_range.end.max(range_end);
                    } else {
                        cluster_entry.dirty_range = Some(range_start..range_end);
                    }
                    remaining_in_cluster[..to_access]
                        .copy_from_slice(&buf[accessed..accessed + to_access]);
                }
            }

            accessed += to_access;
            position_in_cluster += to_access as u32;
            if position_in_cluster >= bytes_per_cluster {
                assert_eq!(position_in_cluster, bytes_per_cluster);
                position_in_cluster = 0;
                match self.fat.next_cluster(cluster)? {
                    Some(next_cluster) => {
                        self.release_cluster(inode, cluster)?;
                        cluster = next_cluster;
                        cluster_entry = self.lock_cluster(cluster)?;
                        access_helper.current_cluster = cluster as u64;
                        access_helper.cluster_index += 1;
                    }
                    None => {
                        break;
                    }
                };
            }
        }

        Ok(accessed as u64)
    }

    fn update_directory_entry(
        &mut self,
        inode: &BaseNode,
        mut update: impl FnMut(&mut DirectoryEntryNormal),
    ) -> Result<(), FileSystemError> {
        let mut sector = self.read_sectors_no_cache(inode.parent_dir_sector() as u32, 1)?;
        let entry_index = inode.parent_dir_index();
        let start = entry_index as usize * DIRECTORY_ENTRY_SIZE as usize;
        let end = start + DIRECTORY_ENTRY_SIZE as usize;

        let mut entry = DirectoryEntry::from_raw(&mut sector[start..end]);
        assert_eq!(entry.state(), DirectoryEntryState::Used);

        let entry = entry.as_normal_mut();
        let current = entry.clone();
        update(entry);

        if current != *entry {
            // write back
            self.write_sectors(inode.parent_dir_sector() as u32, &sector)?;
        }

        Ok(())
    }

    fn add_directory_entry(
        &mut self,
        parent_inode: &DirectoryNode,
        name: &str,
        attributes: FileAttributes,
    ) -> Result<Node, FileSystemError> {
        let (mut normal_entry, long_name_entries) = create_dir_entries(name, attributes);

        // NOTE: here, we perform the following (for dirs)
        // - allocate cluster
        // - create the directory (this may fail, if so, rollback)
        // - create the . and .. entries
        // We could have done it more efficiently, by starting with cluster=0
        // and then allocating and creating directories if its not existent
        // but the issue is that qemu vvfat driver will complain and print some debug messages that
        // cluster 0 is used multiple times, so we are here, making sure the cluster is always
        // valid
        //
        // TODO: probably we don't need this for normal disks, but for now, lets keep it as its
        // easier to use vvfat
        // create the . and .. entries if this is a directory

        let cluster = self
            .fat
            .find_free_cluster()
            .ok_or(FatError::NotEnoughSpace)?;
        self.fat.write_fat_entry(cluster, FatEntry::EndOfChain);
        self.flush_fat()?;
        // let's empty out the first sector only
        // if it's a file, the size is 0 anyway
        // if it's a directory, it will exit since the
        // first direntry will be zero
        self.write_sectors(
            self.first_sector_of_cluster(cluster),
            &vec![0; self.boot_sector.bytes_per_sector() as usize],
        )?;

        normal_entry.first_cluster_lo = (cluster & 0xFFFF) as u16;
        normal_entry.first_cluster_hi = (cluster >> 16) as u16;

        let node = self
            .open_dir_inode(parent_inode)
            .and_then(|mut dir| dir.add_entry(normal_entry.clone(), long_name_entries));

        let node = match node {
            Ok(node) => node,
            e @ Err(_) => {
                // revert fat changes
                self.fat.write_fat_entry(cluster, FatEntry::Free);
                self.flush_fat()?;
                return e;
            }
        };

        assert_eq!(node.name(), name, "node name: {:?}", node.name());

        // create the . and .. entries if this is a directory
        if attributes.directory() {
            assert_eq!(node.start_cluster(), cluster as u64);
            let mut dot_entry = DirectoryEntryNormal {
                short_name: [0x20; 11],
                _nt_reserved: 0,
                ..normal_entry
            };
            dot_entry.short_name[0] = b'.';
            let parent_cluster = parent_inode.start_cluster() as u32;
            let mut dot_dot_entry = DirectoryEntryNormal {
                short_name: [0x20; 11],
                _nt_reserved: 0,
                attributes: file_attribute_to_fat(parent_inode.attributes()),
                creation_time_tenths_of_seconds: 0,
                creation_time: 0,
                creation_date: 0,
                last_access_date: 0,
                first_cluster_lo: (parent_cluster & 0xFFFF) as u16,
                first_cluster_hi: (parent_cluster >> 16) as u16,
                last_modification_time: 0,
                last_modification_date: 0,
                file_size: 0,
            };
            dot_dot_entry.short_name[0] = b'.';
            dot_dot_entry.short_name[1] = b'.';

            let dir_node = match node {
                Node::Directory(ref dir) => dir,
                _ => unreachable!(),
            };

            let mut dir_iter = self.open_dir_inode(dir_node)?;
            let dot_node = dir_iter.add_entry(dot_entry, Vec::new())?;
            let dot_dot_node = dir_iter.add_entry(dot_dot_entry, Vec::new())?;

            assert_eq!(dot_node.name(), ".", "dot node name: {:?}", dot_node.name());
            assert_eq!(
                dot_dot_node.name(),
                "..",
                "dot dot node name: {:?}",
                dot_dot_node.name()
            );
        }

        Ok(node)
    }

    fn set_file_size(&mut self, inode: &mut FileNode, size: u64) -> Result<(), FileSystemError> {
        let bytes_per_cluster = self.boot_sector.bytes_per_cluster() as u64;
        let current_size_in_clusters = (inode.size() + bytes_per_cluster - 1) / bytes_per_cluster;
        let new_size_in_clusters = (size + bytes_per_cluster - 1) / bytes_per_cluster;

        // at least 1 cluster at any point
        let current_size_in_clusters = current_size_in_clusters.max(1);
        let new_size_in_clusters = new_size_in_clusters.max(1);

        if new_size_in_clusters != current_size_in_clusters {
            // update fat references
            let to_keep = new_size_in_clusters.min(current_size_in_clusters);

            let mut current_cluster = inode.start_cluster() as u32;

            // if we won't keep anything, then don't loop
            for _ in 0..to_keep.saturating_sub(1) {
                let next_cluster = self
                    .fat
                    .next_cluster(current_cluster)?
                    .expect("next cluster");
                current_cluster = next_cluster;
            }

            let mut last_cluster = current_cluster;

            if current_size_in_clusters > new_size_in_clusters {
                // deleting old clusters
                let to_delete = current_size_in_clusters - new_size_in_clusters;
                let mut clusters = Vec::with_capacity(to_delete as usize);

                for _ in 0..to_delete {
                    let next_cluster = self
                        .fat
                        .next_cluster(current_cluster)?
                        .expect("next cluster");
                    clusters.push(next_cluster);
                    current_cluster = next_cluster;
                }

                // move backwards
                for cluster in clusters.into_iter().rev() {
                    self.fat.write_fat_entry(cluster, FatEntry::Free);
                }

                // mark the current cluster as last
                self.fat.write_fat_entry(last_cluster, FatEntry::EndOfChain);
            } else {
                // adding new clusters

                let to_add = new_size_in_clusters - current_size_in_clusters;

                for _ in 0..to_add {
                    let new_cluster = self
                        .fat
                        .find_free_cluster()
                        .ok_or(FatError::NotEnoughSpace)?;

                    self.fat
                        .write_fat_entry(last_cluster, FatEntry::Next(new_cluster));
                    // reserve
                    self.fat.write_fat_entry(new_cluster, FatEntry::EndOfChain);

                    last_cluster = new_cluster;
                }
            }
        }

        inode.set_size(size);

        Ok(())
    }
}

impl FileSystem for Mutex<FatFilesystem> {
    fn open_root(&self) -> Result<DirectoryNode, FileSystemError> {
        self.lock().open_root_dir_inode()
    }

    fn read_dir(
        &self,
        inode: &DirectoryNode,
        handler: &mut dyn FnMut(Node) -> DirTreverse,
    ) -> Result<(), FileSystemError> {
        for node in self.lock().open_dir_inode(inode)? {
            if let DirTreverse::Stop = handler(node.into()) {
                break;
            }
        }

        Ok(())
    }

    fn treverse_dir(&self, inode: &DirectoryNode, matcher: &str) -> Result<Node, FileSystemError> {
        for node in self.lock().open_dir_inode(inode)? {
            if node.matches(matcher) {
                return Ok(node.into());
            }
        }

        Err(FileSystemError::FileNotFound)
    }

    fn create_node(
        &self,
        parent: &DirectoryNode,
        name: &str,
        attributes: FileAttributes,
    ) -> Result<Node, FileSystemError> {
        self.lock().add_directory_entry(parent, name, attributes)
    }

    fn read_file(
        &self,
        inode: &FileNode,
        position: u64,
        buf: &mut [u8],
        access_helper: &mut AccessHelper,
    ) -> Result<u64, FileSystemError> {
        assert!(position <= u32::MAX as u64);
        self.lock().read_write_file(
            inode,
            position as u32,
            FileAccessBuffer::Read(buf),
            access_helper,
        )
    }

    fn write_file(
        &self,
        inode: &mut FileNode,
        position: u64,
        buf: &[u8],
        access_helper: &mut AccessHelper,
    ) -> Result<u64, FileSystemError> {
        assert!(position <= u32::MAX as u64);

        let mut s = self.lock();

        let current_size = inode.size();
        let new_size = position + buf.len() as u64;

        if new_size > current_size {
            s.set_file_size(inode, new_size)
                .map_err(|_| FileSystemError::CouldNotSetFileLength)?;
        }

        // we seeked past the end of the file
        // extend to the position with zeros
        if position > current_size {
            let extend_size = position - current_size;

            let zeros = vec![0; s.boot_sector.bytes_per_sector() as usize];

            let mut written = 0;
            while written < extend_size {
                let to_write = zeros.len().min((extend_size - written) as usize);
                s.read_write_file(
                    inode,
                    (current_size + written) as u32,
                    FileAccessBuffer::Write(&zeros[..to_write]),
                    access_helper,
                )?;
                written += to_write as u64;
            }
        }

        s.read_write_file(
            inode,
            position as u32,
            FileAccessBuffer::Write(buf),
            access_helper,
        )
    }

    fn flush_file(
        &self,
        inode: &mut FileNode,
        access_helper: &mut AccessHelper,
    ) -> Result<(), FileSystemError> {
        self.lock()
            .flush_cluster(inode, access_helper.current_cluster as u32)
    }

    fn close_file(
        &self,
        inode: &FileNode,
        access_helper: AccessHelper,
    ) -> Result<(), FileSystemError> {
        self.lock()
            .release_cluster(inode, access_helper.current_cluster as u32)
    }

    fn set_file_size(&self, inode: &mut FileNode, size: u64) -> Result<(), FileSystemError> {
        let mut s = self.lock();

        s.set_file_size(inode, size)?;
        s.flush_fat()?;
        s.update_directory_entry(inode, |entry| {
            entry.file_size = inode.size() as u32;
        })?;

        Ok(())
    }

    fn unmount(self: Arc<Self>) {
        let mut s = self.lock();
        s.flush_fat().expect("flush fat");

        for cluster in s.cluster_cache.release_all().into_values() {
            if let Some(dirty_range) = cluster.dirty_range {
                s.flush_cluster_dirty_range(&cluster.data, cluster.cluster, dirty_range)
                    .expect("flush cluster dirty range");
            }
        }
    }
}
