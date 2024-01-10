use core::{fmt, mem};

use alloc::vec::Vec;

use crate::{fs, memory_management::virtual_memory_mapper};

#[derive(Debug)]
pub enum ElfLoadError {
    InvalidMagic,
    FileSystemError(fs::FileSystemError),
    InvalidElfOrNotSupported,
    UnexpectedEndOfFile,
}

impl From<fs::FileSystemError> for ElfLoadError {
    fn from(e: fs::FileSystemError) -> Self {
        Self::FileSystemError(e)
    }
}

#[allow(dead_code)]
mod consts {
    pub const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
    pub const ABI_SYSV: u8 = 0;
    pub const BITS_32: u8 = 1;
    pub const BITS_64: u8 = 2;
    pub const ENDIANNESS_LITTLE: u8 = 1;
    pub const ENDIANNESS_BIG: u8 = 2;

    pub const ELF_TYPE_RELOCATABLE: u16 = 1;
    pub const ELF_TYPE_EXECUTABLE: u16 = 2;
    pub const ELF_TYPE_SHARED: u16 = 3;

    pub const ELF_MACHINE_X86: u16 = 3;
    pub const ELF_MACHINE_X86_64: u16 = 62;

    pub const PROG_FLAG_EXE: u32 = 0x1;
    pub const PROG_FLAG_WRITE: u32 = 0x2;
    pub const PROG_FLAG_READ: u32 = 0x4;
}

pub fn to_virtual_memory_flags(flags: u32) -> u64 {
    // 0 means read-only
    let mut vm_flags = 0;

    if flags & consts::PROG_FLAG_WRITE != 0 {
        vm_flags |= virtual_memory_mapper::flags::PTE_WRITABLE;
    }
    if flags & consts::PROG_FLAG_EXE != 0 {
        // TODO: add support for executable pages
    }
    vm_flags
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
struct ElfHeaderBase {
    magic: [u8; 4],
    bits: u8,
    endianness: u8,
    version: u8,
    abi: u8,
    abi_version: u8,
    _pad: [u8; 7],
    elf_type: u16,
    machine: u16,
    elf_version: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
struct ElfHeader32 {
    entry: u32,
    program_header_offset: u32,
    section_header_offset: u32,
    flags: u32,
    header_size: u16,
    program_header_entry_size: u16,
    program_header_entry_count: u16,
    section_header_entry_size: u16,
    section_header_entry_count: u16,
    section_header_string_table_index: u16,
}
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
struct ElfHeader64 {
    entry: u64,
    program_header_offset: u64,
    section_header_offset: u64,
    flags: u32,
    header_size: u16,
    program_header_entry_size: u16,
    program_header_entry_count: u16,
    section_header_entry_size: u16,
    section_header_entry_count: u16,
    section_header_string_table_index: u16,
}

#[derive(Copy, Clone)]
union ElfHeaderUnion {
    header32: ElfHeader32,
    header64: ElfHeader64,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct ElfHeader {
    base: ElfHeaderBase,
    header: ElfHeaderUnion,
}

#[allow(dead_code)]
impl ElfHeader {
    fn is_valid_and_supported(&self) -> bool {
        if (self.base.bits != consts::BITS_32 && self.base.bits != consts::BITS_64)
            || self.base.endianness != consts::ENDIANNESS_LITTLE
            || self.base.version != 1
            || self.base.abi != consts::ABI_SYSV
            || self.base.abi_version != 0
            || (self.base.elf_type != consts::ELF_TYPE_EXECUTABLE
                && self.base.elf_type != consts::ELF_TYPE_SHARED)
            || self.base.machine != consts::ELF_MACHINE_X86_64
            || self.base.elf_version != 1
        {
            return false;
        }
        true
    }

    fn is_elf64(&self) -> bool {
        self.base.bits == consts::BITS_64
    }

    fn is_little_endian(&self) -> bool {
        self.base.endianness == consts::ENDIANNESS_LITTLE
    }

    fn entry(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.entry }
        } else {
            unsafe { self.header.header32.entry as u64 }
        }
    }

    fn program_header_offset(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.program_header_offset }
        } else {
            unsafe { self.header.header32.program_header_offset as u64 }
        }
    }

    fn program_header_entry_size(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.program_header_entry_size as u64 }
        } else {
            unsafe { self.header.header32.program_header_entry_size as u64 }
        }
    }

    fn program_header_entry_count(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.program_header_entry_count as u64 }
        } else {
            unsafe { self.header.header32.program_header_entry_count as u64 }
        }
    }

    fn section_header_offset(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.section_header_offset }
        } else {
            unsafe { self.header.header32.section_header_offset as u64 }
        }
    }

    fn size_of_header(&self) -> u64 {
        if self.is_elf64() {
            unsafe { self.header.header64.header_size as u64 }
        } else {
            unsafe { self.header.header32.header_size as u64 }
        }
    }
}

impl fmt::Debug for ElfHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ElfHeader");
        s.field("base", &self.base);
        if self.is_elf64() {
            s.field("header64", unsafe { &self.header.header64 });
        } else {
            s.field("header32", unsafe { &self.header.header32 });
        }
        s.finish()
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ElfProgramType {
    // Unused
    Null,
    // Loadable segment
    Load,
    // Dynamic linking information
    Dynamic,
    // Program interpreter path
    Interpreter,
    // Auxiliary information
    Note,
    // Reserved
    Shlib,
    // Entry for header table itself
    ProgramHeader,
    // Thread-local storage template
    ThreadLocalStorage,
    OsSpecific(u32),
    ProcessorSpecific(u32),
}

impl ElfProgramType {
    pub fn from_u32(ty: u32) -> Self {
        match ty {
            0 => Self::Null,
            1 => Self::Load,
            2 => Self::Dynamic,
            3 => Self::Interpreter,
            4 => Self::Note,
            5 => Self::Shlib,
            6 => Self::ProgramHeader,
            7 => Self::ThreadLocalStorage,
            0x60000000..=0x6fffffff => Self::OsSpecific(ty),
            0x70000000..=0x7fffffff => Self::ProcessorSpecific(ty),
            _ => Self::Null,
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ElfProgram32 {
    // Type of segment
    ty: u32,
    // File offset where segment is located, in bytes
    offset: u32,
    // Virtual address of beginning of segment
    virtual_address: u32,
    // Physical address of beginning of segment (OS-specific)
    physical_address: u32,
    // Num. of bytes in file image of segment (may be zero)
    file_size: u32,
    // Num. of bytes in mem image of segment (may be zero)
    mem_size: u32,
    // Segment flags
    flags: u32,
    // Segment alignment constraint
    alignment: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct ElfProgram64 {
    // Type of segment
    ty: u32,
    // Segment flags
    flags: u32,
    // File offset where segment is located, in bytes
    offset: u64,
    // Virtual address of beginning of segment
    virtual_address: u64,
    // Physical address of beginning of segment (OS-specific)
    physical_address: u64,
    // Num. of bytes in file image of segment (may be zero)
    file_size: u64,
    // Num. of bytes in mem image of segment (may be zero)
    mem_size: u64,
    // Segment alignment constraint
    alignment: u64,
}

#[derive(Clone, Copy)]
pub enum ElfProgram {
    Program32(ElfProgram32),
    Program64(ElfProgram64),
}

impl ElfProgram {
    pub fn load(
        file: &mut fs::File,
        is_elf64: bool,
        entry_size: u64,
    ) -> Result<Self, ElfLoadError> {
        if is_elf64 {
            if entry_size != mem::size_of::<ElfProgram64>() as u64 {
                return Err(ElfLoadError::InvalidElfOrNotSupported);
            }
            let mut header_bytes = [0u8; mem::size_of::<ElfProgram64>()];
            if file.read_file(&mut header_bytes)? != header_bytes.len() as u64 {
                return Err(ElfLoadError::UnexpectedEndOfFile);
            }
            let program = unsafe { &*(header_bytes.as_ptr() as *const ElfProgram64) };
            Ok(Self::Program64(*program))
        } else {
            if entry_size != mem::size_of::<ElfProgram32>() as u64 {
                return Err(ElfLoadError::InvalidElfOrNotSupported);
            }
            let mut header_bytes = [0u8; mem::size_of::<ElfProgram32>()];
            if file.read_file(&mut header_bytes)? != header_bytes.len() as u64 {
                return Err(ElfLoadError::UnexpectedEndOfFile);
            }
            let program = unsafe { &*(header_bytes.as_ptr() as *const ElfProgram32) };
            Ok(Self::Program32(*program))
        }
    }

    pub fn ty(&self) -> ElfProgramType {
        let ty_u32 = match self {
            Self::Program32(p) => p.ty,
            Self::Program64(p) => p.ty,
        };

        ElfProgramType::from_u32(ty_u32)
    }

    pub fn offset(&self) -> u64 {
        match self {
            Self::Program32(p) => p.offset as u64,
            Self::Program64(p) => p.offset,
        }
    }

    pub fn virtual_address(&self) -> u64 {
        match self {
            Self::Program32(p) => p.virtual_address as u64,
            Self::Program64(p) => p.virtual_address,
        }
    }

    pub fn physical_address(&self) -> u64 {
        match self {
            Self::Program32(p) => p.physical_address as u64,
            Self::Program64(p) => p.physical_address,
        }
    }

    pub fn file_size(&self) -> u64 {
        match self {
            Self::Program32(p) => p.file_size as u64,
            Self::Program64(p) => p.file_size,
        }
    }

    pub fn mem_size(&self) -> u64 {
        match self {
            Self::Program32(p) => p.mem_size as u64,
            Self::Program64(p) => p.mem_size,
        }
    }

    pub fn flags(&self) -> u32 {
        match self {
            Self::Program32(p) => p.flags,
            Self::Program64(p) => p.flags,
        }
    }

    pub fn alignment(&self) -> u64 {
        match self {
            Self::Program32(p) => p.alignment as u64,
            Self::Program64(p) => p.alignment,
        }
    }
}

impl fmt::Debug for ElfProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElfProgram")
            .field("ty", &self.ty())
            .field("flags", &self.flags())
            .field("offset", &self.offset())
            .field("virtual_address", &self.virtual_address())
            .field("physical_address", &self.physical_address())
            .field("file_size", &self.file_size())
            .field("mem_size", &self.mem_size())
            .field("alignment", &self.alignment())
            .finish()
    }
}

#[derive(Debug)]
pub struct Elf {
    header: ElfHeader,
    prg_headers: Vec<ElfProgram>,
}

impl Elf {
    pub fn load(file: &mut fs::File) -> Result<Self, ElfLoadError> {
        // take the largest
        let mut header = [0u8; mem::size_of::<ElfHeader>()];
        if file.read_file(&mut header)? != header.len() as u64 {
            return Err(ElfLoadError::UnexpectedEndOfFile);
        }
        let header = unsafe { &*(header.as_ptr() as *const ElfHeader) };

        if &header.base.magic != consts::ELF_MAGIC {
            return Err(ElfLoadError::InvalidMagic);
        }
        if !header.is_valid_and_supported() {
            return Err(ElfLoadError::InvalidElfOrNotSupported);
        }
        file.seek(header.program_header_offset())?;
        let mut program_headers = Vec::with_capacity(header.program_header_entry_count() as usize);

        for _ in 0..header.program_header_entry_count() {
            let program =
                ElfProgram::load(file, header.is_elf64(), header.program_header_entry_size())?;
            program_headers.push(program);
        }

        Ok(Self {
            header: *header,
            prg_headers: program_headers,
        })
    }

    pub fn entry_point(&self) -> u64 {
        self.header.entry()
    }

    pub fn program_headers(&self) -> &[ElfProgram] {
        &self.prg_headers
    }
}
