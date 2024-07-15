mod display;

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec::Vec,
};
use tracing::trace;

#[derive(Debug, Clone)]
pub enum AmlParseError {
    UnexpectedEndOfCode,
    InvalidPkgLengthLead,
    InvalidTermArgInPackage,
    RemainingBytes(usize),
    CannotMoveBackward,
    InvalidTarget(u8),
    NameObjectNotContainingDataObject,
    UnalignedFieldElementOffset,
    InvalidAccessType,
    InvalidExtendedAttrib(u8),
    InvalidFieldUpdateRule,
}

pub fn parse_aml(code: &[u8]) -> Result<AmlCode, AmlParseError> {
    let mut methods = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut parser = Parser {
        code,
        pos: 0,
        state: State::new(&mut methods, &mut names),
    };
    parser.parse_root()
}

#[derive(Debug, Clone)]
pub struct AmlCode {
    pub(super) term_list: Vec<AmlTerm>,
}

#[derive(Debug, Clone)]
pub enum IntegerData {
    ConstZero,
    ConstOne,
    ConstOnes,
    ByteConst(u8),
    WordConst(u16),
    DWordConst(u32),
    QWordConst(u64),
}

#[allow(dead_code)]
impl IntegerData {
    #[inline]
    pub fn as_u8(&self) -> Option<u8> {
        match self {
            Self::ByteConst(byte) => Some(*byte),
            Self::ConstZero => Some(0),
            Self::ConstOne => Some(1),
            Self::ConstOnes => Some(0xFF),
            _ => None,
        }
    }

    #[inline]
    pub fn as_u16(&self) -> Option<u16> {
        match self {
            Self::WordConst(word) => Some(*word),
            Self::ConstOnes => Some(0xFFFF),
            _ => self.as_u8().map(|byte| byte.into()),
        }
    }

    #[inline]
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::DWordConst(dword) => Some(*dword),
            Self::ConstOnes => Some(0xFFFFFFFF),
            _ => self.as_u16().map(|word| word.into()),
        }
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        match self {
            Self::QWordConst(qword) => *qword,
            Self::ConstOnes => 0xFFFFFFFFFFFFFFFF,
            _ => self
                .as_u32()
                .map(|dword| dword.into())
                .expect("Can't fail, all branches are covered"),
        }
    }
}

/// DataObject representation as it is in the AML, which may contain expressions
/// that need to be evaluated at runtime
///
/// For final result, see [DataObject][super::execution::DataObject]
#[derive(Debug, Clone)]
pub enum UnresolvedDataObject {
    Integer(IntegerData),
    Buffer(Buffer),
    Package(u8, Vec<PackageElement<UnresolvedDataObject>>),
    VarPackage(Box<TermArg>, Vec<PackageElement<UnresolvedDataObject>>),
    String(String),
    EisaId(String),
}

/// `D` is the type of data object, it can be [UnresolvedDataObject] or [DataObject][super::execution::DataObject] depending
/// on the state, either parsed program or executed and returned result
#[derive(Debug, Clone)]
pub enum PackageElement<D> {
    DataObject(D),
    Name(String),
}

#[allow(dead_code)]
impl<T> PackageElement<T> {
    pub fn as_data(&self) -> Option<&T> {
        match self {
            Self::DataObject(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub(super) size: Box<TermArg>,
    pub(super) data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum AmlTerm {
    Scope(ScopeObj),
    Region(RegionObj),
    Field(FieldDef),
    IndexField(IndexFieldDef),
    Device(ScopeObj),
    Processor(ProcessorDeprecated),
    PowerResource(PowerResource),
    Method(MethodObj),
    NameObj(String, UnresolvedDataObject),
    Alias(String, String),
    ToHexString(TermArg, Box<Target>),
    ToBuffer(TermArg, Box<Target>),
    ToDecimalString(TermArg, Box<Target>),
    ToInteger(TermArg, Box<Target>),
    Mid(TermArg, TermArg, TermArg, Box<Target>),
    Add(TermArg, TermArg, Box<Target>),
    Concat(TermArg, TermArg, Box<Target>),
    Subtract(TermArg, TermArg, Box<Target>),
    Multiply(TermArg, TermArg, Box<Target>),
    Divide(TermArg, TermArg, Box<Target>, Box<Target>),
    ShiftLeft(TermArg, TermArg, Box<Target>),
    ShiftRight(TermArg, TermArg, Box<Target>),
    And(TermArg, TermArg, Box<Target>),
    Nand(TermArg, TermArg, Box<Target>),
    Or(TermArg, TermArg, Box<Target>),
    Nor(TermArg, TermArg, Box<Target>),
    Xor(TermArg, TermArg, Box<Target>),
    Not(TermArg, Box<Target>),
    SizeOf(Box<Target>),
    Store(TermArg, Box<Target>),
    RefOf(Box<Target>),
    Increment(Box<Target>),
    Decrement(Box<Target>),
    While(PredicateBlock),
    If(PredicateBlock),
    Else(Vec<AmlTerm>),
    Noop,
    Return(TermArg),
    Break,
    LAnd(TermArg, TermArg),
    LOr(TermArg, TermArg),
    LNot(TermArg),
    LNotEqual(TermArg, TermArg),
    LLessEqual(TermArg, TermArg),
    LGreaterEqual(TermArg, TermArg),
    LEqual(TermArg, TermArg),
    LGreater(TermArg, TermArg),
    LLess(TermArg, TermArg),
    FindSetLeftBit(TermArg, Box<Target>),
    FindSetRightBit(TermArg, Box<Target>),
    DerefOf(TermArg),
    ConcatRes(TermArg, TermArg, Box<Target>),
    Mod(TermArg, TermArg, Box<Target>),
    Notify(Box<Target>, TermArg),
    Index(TermArg, TermArg, Box<Target>),
    Mutex(String, u8),
    Event(String),
    CondRefOf(Box<Target>, Box<Target>),
    CreateFieldOp(TermArg, TermArg, TermArg, String),
    Acquire(Box<Target>, u16),
    Signal(Box<Target>),
    Wait(Box<Target>, TermArg),
    Reset(Box<Target>),
    Release(Box<Target>),
    Stall(TermArg),
    Sleep(TermArg),
    CreateDWordField(TermArg, TermArg, String),
    CreateWordField(TermArg, TermArg, String),
    CreateByteField(TermArg, TermArg, String),
    CreateBitField(TermArg, TermArg, String),
    CreateQWordField(TermArg, TermArg, String),
    MethodCall(String, Vec<TermArg>),
    ObjectType(Box<Target>),
}

#[derive(Debug, Clone)]
pub enum TermArg {
    Expression(Box<AmlTerm>),
    DataObject(UnresolvedDataObject),
    Arg(u8),
    Local(u8),
    Name(String),
}

#[derive(Debug, Clone)]
pub enum Target {
    None,
    Arg(u8),
    Local(u8),
    Name(String),
    Debug,
    DerefOf(TermArg),
    RefOf(Box<Target>),
    Index(TermArg, TermArg, Box<Target>),
}

#[derive(Debug, Clone)]
pub struct ScopeObj {
    pub(super) name: String,
    pub(super) term_list: Vec<AmlTerm>,
}

impl ScopeObj {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;

        trace!("scope name: {}", name);
        let term_list = inner.parse_term_list()?;
        inner.check_empty()?;
        Ok(Self { name, term_list })
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::upper_case_acronyms)]
#[allow(non_camel_case_types)]
pub enum RegionSpace {
    SystemMemory,
    SystemIO,
    PCI_Config,
    EmbeddedControl,
    SMBus,
    SystemCMOS,
    PciBarTarget,
    IPMI,
    GeneralPurposeIO,
    GenericSerialBus,
    PCC,
    Other(u8),
}

impl From<u8> for RegionSpace {
    fn from(space: u8) -> Self {
        match space {
            0 => Self::SystemMemory,
            1 => Self::SystemIO,
            2 => Self::PCI_Config,
            3 => Self::EmbeddedControl,
            4 => Self::SMBus,
            5 => Self::SystemCMOS,
            6 => Self::PciBarTarget,
            7 => Self::IPMI,
            8 => Self::GeneralPurposeIO,
            9 => Self::GenericSerialBus,
            10 => Self::PCC,
            _ => Self::Other(space),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionObj {
    pub(super) name: String,
    pub(super) region_space: RegionSpace,
    pub(super) region_offset: TermArg,
    pub(super) region_length: TermArg,
}

impl RegionObj {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let name = parser.parse_name()?;
        trace!("region name: {}", name);
        let region_space = parser.get_next_byte()?.into();
        let region_offset = parser.parse_term_arg()?;
        trace!("region offset: {:?}", region_offset);
        let region_length = parser.parse_term_arg()?;
        trace!("region length: {:?}", region_length);
        Ok(Self {
            name,
            region_space,
            region_offset,
            region_length,
        })
    }
}

#[derive(Debug, Clone)]
pub enum AccessType {
    Any,
    Byte,
    Word,
    DWord,
    QWord,
    Buffer,
}

impl TryFrom<u8> for AccessType {
    type Error = AmlParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value & 0b1111 {
            0 => Ok(Self::Any),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Word),
            3 => Ok(Self::DWord),
            4 => Ok(Self::QWord),
            5 => Ok(Self::Buffer),
            _ => Err(AmlParseError::InvalidAccessType),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FieldUpdateRule {
    Preserve,
    WriteAsOnes,
    WriteAsZeros,
}

impl TryFrom<u8> for FieldUpdateRule {
    type Error = AmlParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value & 0b111 {
            0 => Ok(Self::Preserve),
            1 => Ok(Self::WriteAsOnes),
            2 => Ok(Self::WriteAsZeros),
            _ => Err(AmlParseError::InvalidFieldUpdateRule),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub(super) name: String,
    pub(super) access_type: AccessType,
    pub(super) need_lock: bool,
    pub(super) update_rule: FieldUpdateRule,
    pub(super) fields: Vec<FieldElement>,
}

impl FieldDef {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("field name: {}", name);
        let (flags, field_list) = inner.parse_fields_list_and_flags()?;

        let access_type = flags.try_into()?;
        let need_lock = (flags & (1 << 4)) != 0;
        let update_rule = (flags >> 5).try_into()?;

        Ok(Self {
            name,
            access_type,
            need_lock,
            update_rule,
            fields: field_list,
        })
    }
}

#[derive(Debug, Clone)]
pub struct IndexFieldDef {
    pub(super) name: String,
    pub(super) index_name: String,
    pub(super) access_type: AccessType,
    pub(super) need_lock: bool,
    pub(super) update_rule: FieldUpdateRule,
    pub(super) fields: Vec<FieldElement>,
}

impl IndexFieldDef {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("index-field name: {}", name);
        let index_name = inner.parse_name()?;
        trace!("index-field index_name: {}", index_name);
        let (flags, field_list) = inner.parse_fields_list_and_flags()?;

        let access_type = flags.try_into()?;
        let need_lock = (flags & (1 << 4)) != 0;
        let update_rule = (flags >> 5).try_into()?;

        Ok(Self {
            name,
            index_name,
            access_type,
            need_lock,
            update_rule,
            fields: field_list,
        })
    }
}

#[derive(Debug, Clone)]
pub enum AccessAttrib {
    /// Special variant that only prints the `u8` value, doesn't have name
    ByteValue(u8),

    Bytes(u8),
    RawBytes(u8),
    RawProcessBytes(u8),
    Quick,
    SendRecv,
    Byte,
    Word,
    Block,
    ProcessCall,
    BlockProcessCall,
}

#[derive(Debug, Clone)]
pub enum FieldConnection {
    Buffer(Buffer),
    Name(String),
}

#[derive(Debug, Clone)]
pub enum FieldElement {
    Offset(usize),
    Named(String, usize),
    Access(AccessType, AccessAttrib),
    Connection(FieldConnection),
}

#[derive(Debug, Clone)]
pub struct MethodObj {
    pub name: String,
    pub flags: u8,
    pub term_list: Vec<AmlTerm>,
}

impl MethodObj {
    fn arg_count(&self) -> usize {
        (self.flags & 0b111) as usize
    }

    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("method name: {}", name);
        let flags = inner.get_next_byte()?;
        trace!("method flags: {:x}", flags);
        let term_list = inner.parse_term_list()?;
        inner.check_empty()?;

        Ok(Self {
            name,
            flags,
            term_list,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PredicateBlock {
    predicate: TermArg,
    term_list: Vec<AmlTerm>,
}

impl PredicateBlock {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;

        let predicate = inner.parse_term_arg()?;
        trace!("pred predicate: {:?}", predicate);
        let term_list = inner.parse_term_list()?;
        inner.check_empty()?;

        Ok(Self {
            predicate,
            term_list,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProcessorDeprecated {
    pub(super) name: String,
    pub(super) unk1: u8,
    pub(super) unk2: u32,
    pub(super) unk3: u8,
    pub(super) term_list: Vec<AmlTerm>,
}

impl ProcessorDeprecated {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("processor name: {}", name);
        let unk1 = inner.get_next_byte()?;
        trace!("processor unk1: {:x}", unk1);
        let unk2 = u32::from_le_bytes([
            inner.get_next_byte()?,
            inner.get_next_byte()?,
            inner.get_next_byte()?,
            inner.get_next_byte()?,
        ]);
        trace!("processor unk2: {:x}", unk2);
        let unk3 = inner.get_next_byte()?;
        trace!("processor unk3: {:x}", unk3);
        let term_list = inner.parse_term_list()?;
        inner.check_empty()?;
        Ok(Self {
            name,
            unk1,
            unk2,
            unk3,
            term_list,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PowerResource {
    pub(super) name: String,
    pub(super) system_level: u8,
    pub(super) resource_order: u16,
    pub(super) term_list: Vec<AmlTerm>,
}

impl PowerResource {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("power-resource name: {}", name);
        let system_level = inner.get_next_byte()?;
        trace!("power-resource system_level: {:x}", system_level);
        let resource_order = u16::from_le_bytes([inner.get_next_byte()?, inner.get_next_byte()?]);
        trace!("power-resource resource_order: {:x}", resource_order);
        let term_list = inner.parse_term_list()?;
        inner.check_empty()?;
        Ok(Self {
            name,
            system_level,
            resource_order,
            term_list,
        })
    }
}

type StateMethodsList<'a> = &'a mut BTreeMap<String, usize>;
type StateNamesList<'a> = &'a mut BTreeSet<String>;

/// inner state of the parser to store information about the current scope/position
#[derive(Debug)]
struct State<'a> {
    /// Shared state all method names
    methods: StateMethodsList<'a>,
    /// all found names (aliases, fields, etc.)
    names: StateNamesList<'a>,
}

impl<'a> State<'a> {
    fn new(methods: StateMethodsList<'a>, names: StateNamesList<'a>) -> State<'a> {
        State { methods, names }
    }

    /// Renamed to not be confused with `Clone::clone`
    fn clone_state(&mut self) -> State {
        State {
            methods: self.methods,
            names: self.names,
        }
    }

    fn find_name(&self, name: &str) -> bool {
        trace!("finding name {name:?}, {:?}", self.names);
        let short_name = &name[name.len() - 4..];
        self.names.contains(name) || self.names.contains(short_name)
    }

    fn find_method(&self, name: &str) -> Option<usize> {
        trace!("finding method {name:?}");
        // all methods are shared here, from all scopes
        // we are assuming that methods with similar names have the same number of arguments
        let method_name = &name[name.len() - 4..];
        trace!("methods: {:?}", self.methods);
        self.methods.get(method_name).copied()
    }

    fn add_method(&mut self, name: &str, arg_count: usize) {
        trace!("adding method {name:?}");
        let method_name = &name[name.len() - 4..];
        self.methods.insert(String::from(method_name), arg_count);
    }

    fn add_name(&mut self, name: String) {
        trace!("adding name {name:?}");
        self.names.insert(name);
    }
}

pub struct Parser<'a> {
    code: &'a [u8],
    pos: usize,
    state: State<'a>,
}

impl Parser<'_> {
    fn remaining_bytes(&self) -> usize {
        self.code.len() - self.pos
    }

    fn get_next_byte(&mut self) -> Result<u8, AmlParseError> {
        if self.pos >= self.code.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }
        let byte = self.code[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    fn peek_next_byte(&self) -> Result<u8, AmlParseError> {
        if self.pos >= self.code.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }
        Ok(self.code[self.pos])
    }

    fn forward(&mut self, n: usize) -> Result<(), AmlParseError> {
        if self.pos + n > self.code.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }
        self.pos += n;
        Ok(())
    }

    fn backward(&mut self, n: usize) -> Result<(), AmlParseError> {
        if self.pos == 0 {
            return Err(AmlParseError::CannotMoveBackward);
        }
        self.pos -= n;
        Ok(())
    }

    fn get_pkg_length(&mut self) -> Result<usize, AmlParseError> {
        let lead_byte = self.get_next_byte()?;
        let following_bytes = lead_byte >> 6;

        trace!("pkglen: lead byte: {:x}", lead_byte);

        let mut length: usize;
        if following_bytes == 0 {
            // subtract the bytes used for the length
            return Ok((lead_byte & 0b0011_1111) as usize - 1);
        } else {
            // bits 4-5 must be zero
            if (lead_byte >> 4) & 0b11 != 0 {
                return Err(AmlParseError::InvalidPkgLengthLead);
            }
            length = lead_byte as usize & 0b0000_1111;
        }
        trace!("len now start: {:x}", length);

        for i in 0..following_bytes {
            let byte = self.get_next_byte()?;
            length |= (byte as usize) << (8 * i + 4);
            trace!("len now: {:x}", length);
        }
        // subtract the bytes used for the length
        Ok(length - following_bytes as usize - 1)
    }

    fn get_inner_parser(&mut self) -> Result<Parser, AmlParseError> {
        let pkg_length = self.get_pkg_length()?;
        trace!("inner pkg length: {:x}", pkg_length);

        let inner_parser = Parser {
            code: &self.code[self.pos..self.pos + pkg_length],
            pos: 0,
            state: self.state.clone_state(),
        };
        self.pos += pkg_length;
        Ok(inner_parser)
    }

    /// Renamed to not be confused with `Clone::clone`
    fn clone_parser(&mut self) -> Parser {
        Parser {
            code: self.code,
            pos: self.pos,
            state: self.state.clone_state(),
        }
    }

    fn check_empty(&self) -> Result<(), AmlParseError> {
        if self.pos != self.code.len() {
            return Err(AmlParseError::RemainingBytes(self.code.len() - self.pos));
        }
        Ok(())
    }

    fn parse_term(&mut self) -> Result<AmlTerm, AmlParseError> {
        let byte = self.get_next_byte()?;
        let term = self.try_parse_term(byte)?;

        if let Some(term) = term {
            Ok(term)
        } else {
            todo!("opcode: {:x}", byte)
        }
    }

    fn predict_possible_args(&mut self, expect_data_after: bool, name: &str) -> usize {
        // clone ourselves to search future nodes
        // TODO: reduce allocations
        let mut inner = self.clone_parser();

        let mut n_args = 0;
        // max 7 args
        for _ in 0..7 {
            // filter out impossible cases to be a method argument (taken from ACPICA code),
            // but not exactly the same for simplicity, maybe will need to modify later.
            match inner.parse_term_arg() {
                Ok(TermArg::Name(var_name)) => {
                    // this is an inner expression containing the same name, something like `NAME = NAME + 1`
                    // in that case, this is not a function, and is just a name
                    if name == var_name {
                        return 0;
                    }
                }
                Ok(TermArg::Expression(amlterm)) => match amlterm.as_ref() {
                    AmlTerm::Store(_, _)
                    | AmlTerm::Notify(_, _)
                    | AmlTerm::Release(_)
                    | AmlTerm::Reset(_)
                    | AmlTerm::Signal(_)
                    | AmlTerm::Wait(_, _)
                    | AmlTerm::Sleep(_)
                    | AmlTerm::Stall(_)
                    | AmlTerm::Acquire(_, _)
                    | AmlTerm::CondRefOf(_, _)
                    | AmlTerm::Break
                    | AmlTerm::Return(_)
                    | AmlTerm::Noop
                    | AmlTerm::Else(_)
                    | AmlTerm::If(_)
                    | AmlTerm::While(_)
                    | AmlTerm::Scope(_)
                    | AmlTerm::Region(_)
                    | AmlTerm::Field(_)
                    | AmlTerm::IndexField(_)
                    | AmlTerm::Device(_)
                    | AmlTerm::Processor(_)
                    | AmlTerm::PowerResource(_)
                    | AmlTerm::Method(_)
                    | AmlTerm::NameObj(_, _)
                    | AmlTerm::Alias(_, _)
                    | AmlTerm::ToHexString(_, _)
                    | AmlTerm::ToBuffer(_, _)
                    | AmlTerm::ToDecimalString(_, _)
                    | AmlTerm::ToInteger(_, _)
                    | AmlTerm::Mutex(_, _)
                    | AmlTerm::Event(_)
                    | AmlTerm::CreateDWordField(_, _, _)
                    | AmlTerm::CreateWordField(_, _, _)
                    | AmlTerm::CreateByteField(_, _, _)
                    | AmlTerm::CreateBitField(_, _, _)
                    | AmlTerm::CreateQWordField(_, _, _) => break,
                    AmlTerm::Add(_, _, t)
                    | AmlTerm::Concat(_, _, t)
                    | AmlTerm::Subtract(_, _, t)
                    | AmlTerm::Multiply(_, _, t)
                    | AmlTerm::Divide(_, _, _, t)
                    | AmlTerm::ShiftLeft(_, _, t)
                    | AmlTerm::ShiftRight(_, _, t)
                    | AmlTerm::And(_, _, t)
                    | AmlTerm::Nand(_, _, t)
                    | AmlTerm::Or(_, _, t)
                    | AmlTerm::Nor(_, _, t)
                    | AmlTerm::Xor(_, _, t)
                    | AmlTerm::Not(_, t)
                    | AmlTerm::ConcatRes(_, _, t)
                    | AmlTerm::Mod(_, _, t)
                    | AmlTerm::Index(_, _, t)
                        if !matches!(t.as_ref(), Target::None) =>
                    {
                        // only allow if target is None
                        break;
                    }
                    _ => {}
                },
                Err(e) => {
                    if let AmlParseError::UnexpectedEndOfCode = e {
                        // if we took what is not ours, return it
                        if n_args > 0 && expect_data_after && inner.remaining_bytes() == 0 {
                            n_args -= 1;
                        }
                        return n_args;
                    }
                    break;
                }
                _ => {}
            }

            n_args += 1;
        }
        n_args
    }

    fn try_parse_term(&mut self, opcode: u8) -> Result<Option<AmlTerm>, AmlParseError> {
        trace!("opcode: {:x}", opcode);

        let term = match opcode {
            0x06 => {
                let original_name = self.parse_name()?;
                let aliased_name = self.parse_name()?;
                self.state.add_name(aliased_name.clone());
                self.state.add_name(original_name.clone());

                AmlTerm::Alias(original_name, aliased_name)
            }
            0x08 => {
                let name = self.parse_name()?;
                self.state.add_name(name.clone());

                let mut data_object = self
                    .try_parse_data_object()?
                    .ok_or(AmlParseError::NameObjectNotContainingDataObject)?;

                if let UnresolvedDataObject::Integer(IntegerData::DWordConst(data)) = data_object {
                    if name.contains("ID") {
                        data_object = UnresolvedDataObject::EisaId(Self::parse_eisa_id(data))
                    }
                }

                AmlTerm::NameObj(name, data_object)
            }
            0x10 => AmlTerm::Scope(ScopeObj::parse(self)?),
            0x14 => {
                let method = MethodObj::parse(self)?;
                self.state.add_method(&method.name, method.arg_count());
                AmlTerm::Method(method)
            }
            0x5b => {
                // extra ops
                let inner_opcode = self.get_next_byte()?;

                match inner_opcode {
                    0x01 => AmlTerm::Mutex(self.parse_name()?, self.get_next_byte()?),
                    0x02 => AmlTerm::Event(self.parse_name()?),
                    0x12 => AmlTerm::CondRefOf(self.parse_target()?, self.parse_target()?),
                    0x13 => AmlTerm::CreateFieldOp(
                        self.parse_term_arg()?,
                        self.parse_term_arg()?,
                        self.parse_term_arg()?,
                        self.parse_name()?,
                    ),
                    0x21 => AmlTerm::Stall(self.parse_term_arg()?),
                    0x22 => AmlTerm::Sleep(self.parse_term_arg()?),
                    0x23 => AmlTerm::Acquire(
                        self.parse_target()?,
                        u16::from_le_bytes([self.get_next_byte()?, self.get_next_byte()?]),
                    ),
                    0x24 => AmlTerm::Signal(self.parse_target()?),
                    0x25 => AmlTerm::Wait(self.parse_target()?, self.parse_term_arg()?),
                    0x26 => AmlTerm::Reset(self.parse_target()?),
                    0x27 => AmlTerm::Release(self.parse_target()?),
                    0x80 => AmlTerm::Region(RegionObj::parse(self)?),
                    0x81 => AmlTerm::Field(FieldDef::parse(self)?),
                    0x82 => AmlTerm::Device(ScopeObj::parse(self)?),
                    0x83 => AmlTerm::Processor(ProcessorDeprecated::parse(self)?),
                    0x84 => AmlTerm::PowerResource(PowerResource::parse(self)?),
                    0x86 => AmlTerm::IndexField(IndexFieldDef::parse(self)?),
                    _ => todo!("extra opcode: {:x}", inner_opcode),
                }
            }
            0x70 => AmlTerm::Store(self.parse_term_arg()?, self.parse_target()?),
            0x71 => AmlTerm::RefOf(self.parse_target()?),
            0x72 => AmlTerm::Add(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x73 => AmlTerm::Concat(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x74 => AmlTerm::Subtract(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x75 => AmlTerm::Increment(self.parse_target()?),
            0x76 => AmlTerm::Decrement(self.parse_target()?),
            0x77 => AmlTerm::Multiply(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x78 => AmlTerm::Divide(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
                self.parse_target()?,
            ),
            0x79 => AmlTerm::ShiftLeft(
                self.parse_term_arg_non_method_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7A => AmlTerm::ShiftRight(
                self.parse_term_arg_non_method_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7B => AmlTerm::And(
                self.parse_term_arg_non_method_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7C => AmlTerm::Nand(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7D => AmlTerm::Or(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7E => AmlTerm::Nor(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x7F => AmlTerm::Xor(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x80 => AmlTerm::Not(self.parse_term_arg()?, self.parse_target()?),
            0x81 => AmlTerm::FindSetLeftBit(self.parse_term_arg()?, self.parse_target()?),
            0x82 => AmlTerm::FindSetRightBit(self.parse_term_arg()?, self.parse_target()?),
            0x83 => AmlTerm::DerefOf(self.parse_term_arg()?),
            0x84 => AmlTerm::ConcatRes(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x85 => AmlTerm::Mod(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x86 => AmlTerm::Notify(self.parse_target()?, self.parse_term_arg()?),
            0x87 => AmlTerm::SizeOf(self.parse_target()?),
            0x88 => AmlTerm::Index(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0x8A..=0x8D | 0x8F => {
                let term1 = self.parse_term_arg()?;
                let term2 = self.parse_term_arg()?;
                let name = self.parse_name()?;
                self.state.add_name(name.clone());

                match opcode {
                    0x8A => AmlTerm::CreateDWordField(term1, term2, name),
                    0x8B => AmlTerm::CreateWordField(term1, term2, name),
                    0x8C => AmlTerm::CreateByteField(term1, term2, name),
                    0x8D => AmlTerm::CreateBitField(term1, term2, name),
                    0x8F => AmlTerm::CreateQWordField(term1, term2, name),
                    _ => unreachable!(),
                }
            }
            0x8E => AmlTerm::ObjectType(self.parse_target()?),
            0x90 => AmlTerm::LAnd(self.parse_term_arg()?, self.parse_term_arg()?),
            0x91 => AmlTerm::LOr(self.parse_term_arg()?, self.parse_term_arg()?),
            0x92 => {
                let next_byte = self.peek_next_byte()?;
                match next_byte {
                    0x93 => {
                        self.forward(1)?;
                        AmlTerm::LNotEqual(self.parse_term_arg()?, self.parse_term_arg()?)
                    }
                    0x94 => {
                        self.forward(1)?;
                        AmlTerm::LLessEqual(self.parse_term_arg()?, self.parse_term_arg()?)
                    }
                    0x95 => {
                        self.forward(1)?;
                        AmlTerm::LGreaterEqual(self.parse_term_arg()?, self.parse_term_arg()?)
                    }
                    _ => AmlTerm::LNot(self.parse_term_arg()?),
                }
            }
            0x93 => AmlTerm::LEqual(self.parse_term_arg()?, self.parse_term_arg()?),
            0x94 => AmlTerm::LGreater(self.parse_term_arg()?, self.parse_term_arg()?),
            0x95 => AmlTerm::LLess(self.parse_term_arg()?, self.parse_term_arg()?),
            0x96 => AmlTerm::ToBuffer(self.parse_term_arg()?, self.parse_target()?),
            0x97 => AmlTerm::ToDecimalString(self.parse_term_arg()?, self.parse_target()?),
            0x98 => AmlTerm::ToHexString(self.parse_term_arg()?, self.parse_target()?),
            0x99 => AmlTerm::ToInteger(self.parse_term_arg()?, self.parse_target()?),
            0x9E => AmlTerm::Mid(
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_term_arg()?,
                self.parse_target()?,
            ),
            0xA0 => AmlTerm::If(PredicateBlock::parse(self)?),
            0xA1 => {
                let mut inner = self.get_inner_parser()?;
                let else_list = inner.parse_term_list()?;
                inner.check_empty()?;

                AmlTerm::Else(else_list)
            }
            0xA2 => AmlTerm::While(PredicateBlock::parse(self)?),
            0xA3 => AmlTerm::Noop,
            // parse it as if it's a method arg, this fixes issues of us mis-representing the term as a name
            0xA4 => AmlTerm::Return(self.parse_term_arg_last()?),
            0xA5 => AmlTerm::Break,
            _ => {
                trace!("try parse name");
                // move back once, since we have consumed this byte
                self.backward(1)?;
                let Some(name) = self.try_parse_name()? else {
                    return Ok(None);
                };
                assert!(!name.is_empty());
                let n_args = self
                    .state
                    .find_method(&name)
                    .unwrap_or_else(|| self.predict_possible_args(false, &name));

                let mut args = Vec::new();
                for _ in 0..n_args {
                    args.push(self.parse_term_arg()?);
                }

                AmlTerm::MethodCall(name, args)
            }
        };
        trace!("{:x?}", term);

        Ok(Some(term))
    }

    /// similar to [`Self::parse_term_arg`], but cannot call methods, as in some places method calls are not allowed
    ///
    /// TODO: This should be removed, as in general a method call is a valid term arg, its just
    ///       we break some parts due to us not knowing if a name is a method or not, and prediction predicts wrong and messes up
    ///       This happens for `+` and `>>` and `<<`, cases I have seen and know of bugs in the parsing
    fn parse_term_arg_non_method_arg(&mut self) -> Result<TermArg, AmlParseError> {
        // second arg doesn't matter, not used
        self.parse_term_arg_general(false, true)
    }

    /// similar to [`Self::parse_term_arg`], but doesn't expect to have data after it, i.e. last in statements or something similar
    fn parse_term_arg_last(&mut self) -> Result<TermArg, AmlParseError> {
        self.parse_term_arg_general(true, false)
    }

    fn parse_term_arg(&mut self) -> Result<TermArg, AmlParseError> {
        self.parse_term_arg_general(true, true)
    }

    fn parse_package_element(
        &mut self,
    ) -> Result<PackageElement<UnresolvedDataObject>, AmlParseError> {
        if let Some(data_object) = self.try_parse_data_object()? {
            return Ok(PackageElement::DataObject(data_object));
        }

        if let Some(name) = self.try_parse_name()? {
            return Ok(PackageElement::Name(name));
        }

        Err(AmlParseError::InvalidTermArgInPackage)
    }

    fn parse_eisa_id(id: u32) -> String {
        // 1st 2 hex of the product id
        let byte2 = (id >> 16) & 0xFF;
        // 2nd 2 hex of the product id
        let byte3 = (id >> 24) & 0xFF;

        // 1st 2 hex of the manufacturer id
        let manufacturer_byte0 = id & 0xFF;
        // 2nd 2 hex of the manufacturer id
        let manufacturer_byte1 = (id >> 8) & 0xFF;

        // convert 2 bytes to 3 values, each 5 bits
        let manufacturer_list: [u32; 3] = [
            manufacturer_byte0 >> 2,
            ((manufacturer_byte0 & 0x03) << 3) | (manufacturer_byte1 >> 5),
            manufacturer_byte1 & 0x1F,
        ];

        // convert 3 values to 3 characters
        let manuf: String = manufacturer_list
            .iter()
            .map(|&c| (c + 0x40) as u8 as char)
            .collect();

        format!("{manuf}{byte2:02X}{byte3:02X}")
    }

    fn try_parse_data_object(&mut self) -> Result<Option<UnresolvedDataObject>, AmlParseError> {
        let lead_byte = self.get_next_byte()?;

        let result = match lead_byte {
            0x0 => UnresolvedDataObject::Integer(IntegerData::ConstZero),
            0x1 => UnresolvedDataObject::Integer(IntegerData::ConstOne),
            0xA => {
                let data = self.get_next_byte()?;
                UnresolvedDataObject::Integer(IntegerData::ByteConst(data))
            }
            0xB => {
                let data = u16::from_le_bytes([self.get_next_byte()?, self.get_next_byte()?]);
                UnresolvedDataObject::Integer(IntegerData::WordConst(data))
            }
            0xC => {
                let data = u32::from_le_bytes([
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                ]);
                UnresolvedDataObject::Integer(IntegerData::DWordConst(data))
            }
            0x0D => {
                let mut str = String::new();
                loop {
                    let byte = self.get_next_byte()?;
                    trace!("byte: {:x}", byte);
                    if byte == 0 {
                        break;
                    }
                    str.push(byte as char);
                }
                UnresolvedDataObject::String(str)
            }
            0xE => {
                let data = u64::from_le_bytes([
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                ]);
                UnresolvedDataObject::Integer(IntegerData::QWordConst(data))
            }
            0x11 => {
                let mut inner = self.get_inner_parser()?;
                let buf_size = inner.parse_term_arg()?;
                // no need for `check_empty`, just take all remaining
                UnresolvedDataObject::Buffer(Buffer {
                    size: Box::new(buf_size),
                    data: inner.code[inner.pos..].to_vec(),
                })
            }
            0x12 => {
                let mut inner = self.get_inner_parser()?;
                let package_size = inner.get_next_byte()?;
                trace!("package size: {:x}", package_size);
                let mut package_elements = Vec::new();
                while inner.pos < inner.code.len() {
                    package_elements.push(inner.parse_package_element()?);
                    trace!("package element: {:?}", package_elements.last());
                }
                inner.check_empty()?;
                UnresolvedDataObject::Package(package_size, package_elements)
            }
            0x13 => {
                let mut inner = self.get_inner_parser()?;
                let package_size = inner.parse_term_arg()?;
                let mut package_elements = Vec::new();
                trace!("varpackage size: {:x?}", package_size);
                while inner.pos < inner.code.len() {
                    package_elements.push(inner.parse_package_element()?);
                    trace!("varpackage element: {:?}", package_elements.last());
                }
                inner.check_empty()?;
                UnresolvedDataObject::VarPackage(Box::new(package_size), package_elements)
            }
            0xFF => UnresolvedDataObject::Integer(IntegerData::ConstOnes),
            _ => {
                self.backward(1)?;
                return Ok(None);
            }
        };

        Ok(Some(result))
    }

    fn parse_term_arg_general(
        &mut self,
        can_call_method: bool,
        expect_data_after: bool,
    ) -> Result<TermArg, AmlParseError> {
        if let Some(data_object) = self.try_parse_data_object()? {
            return Ok(TermArg::DataObject(data_object));
        }

        let lead_byte = self.get_next_byte()?;

        if let Some(local) = self.try_parse_local(lead_byte)? {
            Ok(TermArg::Local(local))
        } else if let Some(arg) = self.try_parse_arg(lead_byte)? {
            Ok(TermArg::Arg(arg))
        } else {
            self.backward(1)?;
            if let Some(name) = self.try_parse_name()? {
                assert!(!name.is_empty());
                let option_nargs = self.state.find_method(&name).or_else(|| {
                    if self.state.find_name(&name) {
                        None
                    } else if can_call_method {
                        trace!("predicting possible args for {name}");
                        let possible_args = self.predict_possible_args(expect_data_after, &name);
                        trace!("got possible args: {possible_args} {name}");
                        // if its 0 and we are inside a method call, probably this is just a named variable
                        if possible_args == 0 {
                            self.state.add_name(name.clone());
                            None
                        } else {
                            Some(possible_args)
                        }
                    } else {
                        // we didn't find, the name, and we can't use methods, so assume it's a name
                        self.state.add_name(name.clone());
                        None
                    }
                });
                if let Some(n_args) = option_nargs {
                    let mut args = Vec::new();
                    for _ in 0..n_args {
                        args.push(self.parse_term_arg()?);
                    }

                    Ok(TermArg::Expression(Box::new(AmlTerm::MethodCall(
                        name, args,
                    ))))
                } else {
                    Ok(TermArg::Name(name))
                }
            } else {
                // didn't work for `name`, we need to go forward to be back to where we were before
                self.forward(1)?;

                if let Some(term) = self
                    .try_parse_term(lead_byte)?
                    .map(|term| TermArg::Expression(Box::new(term)))
                {
                    Ok(term)
                } else {
                    todo!("term arg lead byte: {:x}", lead_byte)
                }
            }
        }
    }

    fn try_parse_name(&mut self) -> Result<Option<String>, AmlParseError> {
        let name_char_byte = self.peek_next_byte()?;

        fn parse_name_path(parser: &mut Parser) -> Result<String, AmlParseError> {
            let byte = parser.get_next_byte()?;
            let mut str = String::new();

            if byte == 0 {
                return Ok(str);
            }

            str.push(byte as char);

            // add 3 more
            for _ in 0..3 {
                let byte = parser.get_next_byte()?;
                match byte {
                    b'A'..=b'Z' | b'_' | b'0'..=b'9' => {
                        str.push(byte as char);
                    }
                    _ => panic!("invalid name path char: {:x} so far {str:?}", byte),
                }
            }

            Ok(str)
        }

        trace!("name char byte: {:x}", name_char_byte);

        match name_char_byte {
            0 => {
                self.forward(1)?;
                Ok(Some(String::new()))
            }
            // lead name char
            b'A'..=b'Z' | b'_' => Ok(Some(parse_name_path(self)?)),
            // // digit char
            // b'0'..=b'9' => {}
            // root char
            b'\\' => {
                self.forward(1)?;
                let name = self.parse_name()?;
                Ok(Some(format!("\\{}", name)))
            }
            // parent prefix
            b'^' => {
                let mut str = String::new();
                while self.peek_next_byte()? == b'^' {
                    self.forward(1)?;
                    str.push('^');
                }
                str += &self.parse_name()?;

                Ok(Some(str))
            }
            b'.' => {
                self.forward(1)?;
                let seg1 = parse_name_path(self)?;
                let seg2 = parse_name_path(self)?;
                Ok(Some(format!("{seg1}.{seg2}")))
            }
            b'/' => {
                self.forward(1)?;
                let count = self.get_next_byte()?;
                let mut str = String::new();
                for i in 0..count {
                    str += &parse_name_path(self)?;
                    if i != count - 1 {
                        str += ".";
                    }
                }
                Ok(Some(str))
            }
            _ => Ok(None),
        }
    }

    fn parse_name(&mut self) -> Result<String, AmlParseError> {
        let peek = self.peek_next_byte()?;
        let name = self.try_parse_name()?;

        if let Some(name) = name {
            Ok(name)
        } else {
            todo!("char not valid {:X}", peek)
        }
    }

    fn try_parse_local(&mut self, lead: u8) -> Result<Option<u8>, AmlParseError> {
        match lead {
            0x60..=0x67 => {
                // local0-local7
                Ok(Some(lead - 0x60))
            }
            _ => Ok(None),
        }
    }

    fn try_parse_arg(&mut self, lead: u8) -> Result<Option<u8>, AmlParseError> {
        match lead {
            0x68..=0x6E => {
                // arg0-arg6
                Ok(Some(lead - 0x68))
            }
            _ => Ok(None),
        }
    }

    fn parse_target(&mut self) -> Result<Box<Target>, AmlParseError> {
        let lead_byte = self.peek_next_byte()?;

        let x = match lead_byte {
            0x0 => {
                self.forward(1)?;
                Ok(Target::None)
            }
            0x5b => {
                self.forward(1)?;
                let next_byte = self.get_next_byte()?;
                assert_eq!(next_byte, 0x31);
                Ok(Target::Debug)
            }
            0x71 => {
                // typeref opcode
                panic!("typeref opcode")
            }
            _ => {
                if let Some(local) = self.try_parse_local(lead_byte)? {
                    self.forward(1)?;
                    Ok(Target::Local(local))
                } else if let Some(arg) = self.try_parse_arg(lead_byte)? {
                    self.forward(1)?;
                    Ok(Target::Arg(arg))
                } else if let Some(name) = self.try_parse_name()? {
                    Ok(Target::Name(name))
                } else {
                    self.forward(1)?;
                    if let Some(term) =
                        self.try_parse_term(lead_byte)?.and_then(|term| match term {
                            AmlTerm::Index(term_arg1, term_arg2, target) => {
                                Some(Target::Index(term_arg1, term_arg2, target))
                            }
                            AmlTerm::RefOf(target) => Some(Target::RefOf(target)),
                            AmlTerm::DerefOf(term_arg) => Some(Target::DerefOf(term_arg)),
                            _ => None,
                        })
                    {
                        trace!("mmmm: {:x?}", term);
                        Ok(term)
                    } else {
                        Err(AmlParseError::InvalidTarget(lead_byte))
                    }
                }
            }
        };
        trace!("target: {:x?}", x);
        x.map(Box::new)
    }

    fn parse_term_list(&mut self) -> Result<Vec<AmlTerm>, AmlParseError> {
        let mut term_list = Vec::new();
        while self.pos < self.code.len() {
            let term = self.parse_term()?;
            term_list.push(term);
        }
        if self.remaining_bytes() != 0 {
            return Err(AmlParseError::RemainingBytes(self.remaining_bytes()));
        }
        Ok(term_list)
    }

    fn parse_fields_list_and_flags(mut self) -> Result<(u8, Vec<FieldElement>), AmlParseError> {
        let flags = self.get_next_byte()?;
        trace!("field flags: {:x}", flags);
        let mut field_list = Vec::new();

        let mut fields_pos_bits = 0;

        while self.pos < self.code.len() {
            let lead = self.peek_next_byte()?;

            let field = match lead {
                0 => {
                    self.forward(1)?;
                    let pkg_length = self.get_pkg_length()?;
                    trace!("reserved field element pkg length: {:x}", pkg_length);
                    // add 1 since we are not using it as normal pkg length
                    fields_pos_bits += pkg_length + 1;
                    if fields_pos_bits % 8 != 0 {
                        return Err(AmlParseError::UnalignedFieldElementOffset);
                    }
                    FieldElement::Offset(fields_pos_bits / 8)
                }
                1 => {
                    self.forward(1)?;
                    let access_byte = self.get_next_byte()?;
                    let access_attrib_byte = self.get_next_byte()?;

                    let access_attrib = match access_byte >> 6 {
                        0 => match access_attrib_byte {
                            0x2 => AccessAttrib::Quick,
                            0x4 => AccessAttrib::SendRecv,
                            0x6 => AccessAttrib::Byte,
                            0x8 => AccessAttrib::Word,
                            0xA => AccessAttrib::Block,
                            0xC => AccessAttrib::ProcessCall,
                            0xD => AccessAttrib::BlockProcessCall,
                            _ => AccessAttrib::ByteValue(access_attrib_byte),
                        },
                        1 => AccessAttrib::Bytes(access_attrib_byte),
                        2 => AccessAttrib::RawBytes(access_attrib_byte),
                        3 => AccessAttrib::RawProcessBytes(access_attrib_byte),
                        _ => unreachable!(),
                    };

                    FieldElement::Access(access_byte.try_into()?, access_attrib)
                }
                2 => {
                    self.forward(1)?;

                    let mut clone = self.clone_parser();
                    let data_object = clone.try_parse_data_object()?;
                    let connection_field =
                        if let Some(UnresolvedDataObject::Buffer(buffer)) = data_object {
                            FieldConnection::Buffer(buffer)
                        } else {
                            // didn't work, try a name
                            FieldConnection::Name(self.parse_name()?)
                        };

                    FieldElement::Connection(connection_field)
                }
                3 => {
                    self.forward(1)?;
                    let access_byte = self.get_next_byte()?;
                    let extended_attrib = self.get_next_byte()?;
                    let access_length = self.get_next_byte()?;

                    let access_attrib = match extended_attrib {
                        0xB => AccessAttrib::Bytes(access_length),
                        0xE => AccessAttrib::RawBytes(access_length),
                        0xF => AccessAttrib::RawProcessBytes(access_length),
                        _ => return Err(AmlParseError::InvalidExtendedAttrib(extended_attrib)),
                    };

                    FieldElement::Access(access_byte.try_into()?, access_attrib)
                }
                _ => {
                    let len_now = self.pos;
                    let name = self.parse_name()?;
                    self.state.add_name(name.clone());
                    assert_eq!(self.pos - len_now, 4); // must be a name segment
                    trace!("field element name: {}", name);
                    let pkg_length = self.get_pkg_length()?;
                    trace!("field element pkg length: {:x}", pkg_length);
                    let size_bits = pkg_length + 1;
                    fields_pos_bits += size_bits;
                    // add 1 since we are not using it as normal pkg length
                    FieldElement::Named(name, size_bits)
                }
            };
            field_list.push(field);
        }

        self.check_empty()?;

        Ok((flags, field_list))
    }

    fn parse_root(&mut self) -> Result<AmlCode, AmlParseError> {
        let term_list = self.parse_term_list()?;
        trace!("{:?}", term_list);

        Ok(AmlCode { term_list })
    }
}
