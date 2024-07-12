use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec::Vec,
};
use core::fmt;
use tracing::trace;

#[derive(Debug, Clone)]
pub enum AmlParseError {
    UnexpectedEndOfCode,
    InvalidPkgLengthLead,
    RemainingBytes(usize),
    CannotMoveBackward,
    InvalidTarget(u8),
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
pub enum DataObject {
    ConstZero,
    ConstOne,
    ConstOnes,
    ByteConst(u8),
    WordConst(u16),
    DWordConst(u32),
    QWordConst(u64),
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
    NameObj(String, TermArg),
    Package(u8, Vec<TermArg>),
    VarPackage(TermArg, Vec<TermArg>),
    Alias(String, String),
    String(String),
    Buffer(TermArg, Vec<u8>),
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
    DataObject(DataObject),
    Arg(u8),
    Local(u8),
    Name(String),
    EisaId(String),
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
pub struct RegionObj {
    pub(super) name: String,
    pub(super) region_space: u8,
    pub(super) region_offset: TermArg,
    pub(super) region_length: TermArg,
}

impl RegionObj {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let name = parser.parse_name()?;
        trace!("region name: {}", name);
        let region_space = parser.get_next_byte()?;
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
pub struct FieldDef {
    pub(super) name: String,
    pub(super) flags: u8,
    pub(super) fields: Vec<FieldElement>,
}

impl FieldDef {
    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let mut inner = parser.get_inner_parser()?;
        let name = inner.parse_name()?;
        trace!("field name: {}", name);
        let (flags, field_list) = inner.parse_fields_list_and_flags()?;
        Ok(Self {
            name,
            flags,
            fields: field_list,
        })
    }
}

#[derive(Debug, Clone)]
pub struct IndexFieldDef {
    pub(super) name: String,
    pub(super) index_name: String,
    pub(super) flags: u8,
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
        Ok(Self {
            name,
            index_name,
            flags,
            fields: field_list,
        })
    }
}

#[derive(Debug, Clone)]
pub enum FieldElement {
    Reserved(usize),
    Named(String, usize),
    Access(u8, u8),
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
                    | AmlTerm::Buffer(_, _)
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

                let mut term = self.parse_term_arg()?;

                if let TermArg::DataObject(DataObject::DWordConst(data)) = term {
                    if name.contains("ID") {
                        term = TermArg::EisaId(Self::parse_eisa_id(data))
                    }
                }

                AmlTerm::NameObj(name, term)
            }
            0x0d => {
                let mut str = String::new();
                loop {
                    let byte = self.get_next_byte()?;
                    trace!("byte: {:x}", byte);
                    if byte == 0 {
                        break;
                    }
                    str.push(byte as char);
                }
                AmlTerm::String(str)
            }
            0x10 => AmlTerm::Scope(ScopeObj::parse(self)?),
            0x11 => {
                let mut inner = self.get_inner_parser()?;
                let buf_size = inner.parse_term_arg()?;
                // no need for `check_empty`, just take all remaining
                AmlTerm::Buffer(buf_size, inner.code[inner.pos..].to_vec())
            }
            0x12 => {
                let mut inner = self.get_inner_parser()?;
                let package_size = inner.get_next_byte()?;
                trace!("package size: {:x}", package_size);
                let mut package_elements = Vec::new();
                while inner.pos < inner.code.len() {
                    package_elements.push(inner.parse_term_arg()?);
                    trace!("package element: {:?}", package_elements.last());
                }
                inner.check_empty()?;
                AmlTerm::Package(package_size, package_elements)
            }
            0x13 => {
                let mut inner = self.get_inner_parser()?;
                let package_size = inner.parse_term_arg()?;
                let mut package_elements = Vec::new();
                trace!("varpackage size: {:x?}", package_size);
                while inner.pos < inner.code.len() {
                    package_elements.push(inner.parse_term_arg()?);
                    trace!("varpackage element: {:?}", package_elements.last());
                }
                inner.check_empty()?;
                AmlTerm::VarPackage(package_size, package_elements)
            }
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

    fn parse_term_arg_general(
        &mut self,
        can_call_method: bool,
        expect_data_after: bool,
    ) -> Result<TermArg, AmlParseError> {
        let lead_byte = self.get_next_byte()?;

        let x = match lead_byte {
            0x0 => Ok(TermArg::DataObject(DataObject::ConstZero)),
            0x1 => Ok(TermArg::DataObject(DataObject::ConstOne)),
            0xA => {
                let data = self.get_next_byte()?;
                Ok(TermArg::DataObject(DataObject::ByteConst(data)))
            }
            0xB => {
                let data = u16::from_le_bytes([self.get_next_byte()?, self.get_next_byte()?]);
                Ok(TermArg::DataObject(DataObject::WordConst(data)))
            }
            0xC => {
                let data = u32::from_le_bytes([
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                    self.get_next_byte()?,
                ]);
                Ok(TermArg::DataObject(DataObject::DWordConst(data)))
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
                Ok(TermArg::DataObject(DataObject::QWordConst(data)))
            }
            0xFF => Ok(TermArg::DataObject(DataObject::ConstOnes)),
            _ => {
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
                                let possible_args =
                                    self.predict_possible_args(expect_data_after, &name);
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
        };
        trace!("term arg: {:x?}", x);
        x
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

        while self.pos < self.code.len() {
            let lead = self.peek_next_byte()?;

            let field = match lead {
                0 => {
                    self.forward(1)?;
                    let pkg_length = self.get_pkg_length()?;
                    trace!("reserved field element pkg length: {:x}", pkg_length);
                    // add 1 since we are not using it as normal pkg length
                    FieldElement::Reserved(pkg_length + 1)
                }
                1 => {
                    self.forward(1)?;
                    let access_type = self.get_next_byte()?;
                    let access_attrib = self.get_next_byte()?;

                    FieldElement::Access(access_type, access_attrib)
                }
                2 => todo!("connection field"),
                3 => todo!("extended access field"),
                _ => {
                    let len_now = self.pos;
                    let name = self.parse_name()?;
                    self.state.add_name(name.clone());
                    assert_eq!(self.pos - len_now, 4); // must be a name segment
                    trace!("field element name: {}", name);
                    let pkg_length = self.get_pkg_length()?;
                    trace!("field element pkg length: {:x}", pkg_length);
                    // add 1 since we are not using it as normal pkg length
                    FieldElement::Named(name, pkg_length + 1)
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

// display impls, we are not using `fmt::Display`, since we have a special `depth` to propagate
// we could have used a `fmt::Display` wrapper, which is another approach, not sure which is better.

pub(super) fn display_depth(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        write!(f, "  ")?;
    }
    Ok(())
}

pub(super) fn display_terms(
    term_list: &[AmlTerm],
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    for term in term_list {
        display_depth(f, depth)?;
        display_term(term, f, depth)?;
        writeln!(f)?;
    }
    Ok(())
}

pub(super) fn display_term_arg(
    term_arg: &TermArg,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    match term_arg {
        TermArg::Expression(term) => display_term(term, f, depth),
        TermArg::DataObject(data) => match data {
            DataObject::ConstZero => write!(f, "Zero"),
            DataObject::ConstOne => write!(f, "One"),
            DataObject::ConstOnes => write!(f, "0xFFFFFFFFFFFFFFFF"),
            DataObject::ByteConst(data) => write!(f, "0x{:02X}", data),
            DataObject::WordConst(data) => write!(f, "0x{:04X}", data),
            DataObject::DWordConst(data) => write!(f, "0x{:08X}", data),
            DataObject::QWordConst(data) => write!(f, "0x{:016X}", data),
        },
        TermArg::Arg(arg) => write!(f, "Arg{:x}", arg),
        TermArg::Local(local) => write!(f, "Local{:x}", local),
        TermArg::Name(name) => write!(f, "{}", name),
        TermArg::EisaId(eisa_id) => write!(f, "EisaId ({:?})", eisa_id),
    }
}

fn display_target(target: &Target, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    match target {
        Target::None => write!(f, "None"),
        Target::Arg(arg) => write!(f, "Arg{:x}", arg),
        Target::Local(local) => write!(f, "Local{:x}", local),
        Target::Name(name) => write!(f, "{}", name),
        Target::Debug => write!(f, "Debug"),
        Target::DerefOf(term_arg) => {
            write!(f, "DerefOf (")?;
            display_term_arg(term_arg, f, depth)?;
            write!(f, ")")
        }
        Target::RefOf(target) => {
            write!(f, "RefOf (")?;
            display_target(target, f, depth)?;
            write!(f, ")")
        }
        Target::Index(term_arg1, term_arg2, target) => {
            display_index(term_arg1, term_arg2, target, f, depth)
        }
    }
}

pub(super) fn display_terms_list<'a>(
    terms: impl ExactSizeIterator<Item = &'a TermArg>,
    depth_divider: Option<usize>,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    let len = terms.len();
    for (i, arg) in terms.enumerate() {
        let mut add_depth = 0;
        if let Some(depth_divider) = depth_divider {
            if i % depth_divider == 0 {
                writeln!(f)?;
                display_depth(f, depth + 1)?;
            }
            add_depth = 1;
        }
        display_term_arg(arg, f, depth + add_depth)?;
        if i != len - 1 {
            write!(f, ", ")?;
        }
    }
    Ok(())
}

fn display_call_term_target(
    name: &str,
    args: &[&TermArg],
    targets: &[&Target],
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    write!(f, "{} (", name)?;
    if !args.is_empty() {
        display_terms_list(args.iter().copied(), None, f, depth)?;
        if !targets.is_empty() {
            write!(f, ", ")?;
        }
    }
    for (i, target) in targets.iter().enumerate() {
        display_target(target, f, depth)?;
        if i != targets.len() - 1 {
            write!(f, ", ")?;
        }
    }
    write!(f, ")")
}

fn display_binary_op(
    op: &str,
    arg1: &TermArg,
    arg2: &TermArg,
    target: &Target,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    if !matches!(target, Target::None) {
        display_target(target, f, depth)?;
        write!(f, " = ")?;
    }
    write!(f, "( ")?;
    display_term_arg(arg1, f, depth)?;
    write!(f, " {} ", op)?;
    display_term_arg(arg2, f, depth)?;
    write!(f, " )")
}

fn display_index(
    term1: &TermArg,
    term2: &TermArg,
    target: &Target,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    if !matches!(target, Target::None) {
        display_target(target, f, depth)?;
        write!(f, " = ")?;
    }
    display_term_arg(term1, f, depth)?;
    write!(f, "[")?;
    display_term_arg(term2, f, depth)?;
    write!(f, "]")
}

fn display_scope(scope: &ScopeObj, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    writeln!(f, "({}) {{", scope.name)?;
    display_terms(&scope.term_list, f, depth + 1)?;
    display_depth(f, depth)?;
    writeln!(f, "}}")
}

pub(super) fn display_method(
    method: &MethodObj,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    writeln!(f, "Method ({}, {}) {{", method.name, method.flags)?;
    display_terms(&method.term_list, f, depth + 1)?;
    display_depth(f, depth)?;
    write!(f, "}}")
}

fn display_predicate_block(
    name: &str,
    predicate_block: &PredicateBlock,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    write!(f, "{} (", name)?;
    display_term_arg(&predicate_block.predicate, f, depth)?;
    writeln!(f, ") {{")?;
    display_terms(&predicate_block.term_list, f, depth + 1)?;
    display_depth(f, depth)?;
    write!(f, "}}")
}

pub(super) fn display_fields(
    fields: &[FieldElement],
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    let len = fields.len();
    for (i, field) in fields.iter().enumerate() {
        display_depth(f, depth)?;
        match field {
            FieldElement::Reserved(len) => write!(f, "_Reserved (0x{:02X})", len)?,
            FieldElement::Named(name, len) => write!(f, "{},     (0x{:02X})", name, len)?,
            FieldElement::Access(access_type, access_attrib) => write!(
                f,
                "AccessAs  (0x{:02X}, 0x{:02X})",
                access_type, access_attrib
            )?,
        }
        if i != len - 1 {
            write!(f, ", ")?;
        }
        writeln!(f)?;
    }
    Ok(())
}

fn display_term(term: &AmlTerm, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    match term {
        AmlTerm::Alias(name1, name2) => {
            write!(f, "Alias({}, {})", name1, name2)?;
        }
        AmlTerm::Scope(scope) => {
            write!(f, "Scope ")?;
            display_scope(scope, f, depth)?;
        }
        AmlTerm::Device(scope) => {
            write!(f, "Device ")?;
            display_scope(scope, f, depth)?;
        }
        AmlTerm::Region(region) => {
            write!(f, "Region ({}, {}, ", region.name, region.region_space,)?;
            display_term_arg(&region.region_offset, f, depth)?;
            write!(f, ", ")?;
            display_term_arg(&region.region_length, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::Field(field) => {
            writeln!(f, "Field ({}, {}) {{", field.name, field.flags)?;
            display_fields(&field.fields, f, depth + 1)?;
            display_depth(f, depth)?;
            writeln!(f, "}}")?;
        }
        AmlTerm::IndexField(index_field) => {
            writeln!(
                f,
                "IndexField ({}, {}, {}) {{",
                index_field.name, index_field.index_name, index_field.flags
            )?;
            display_fields(&index_field.fields, f, depth + 1)?;
            display_depth(f, depth)?;
            writeln!(f, "}}")?;
        }
        AmlTerm::Package(size, elements) => {
            write!(f, "Package (0x{:02X}) {{", size)?;
            display_terms_list(elements.iter(), Some(4), f, depth)?;
            writeln!(f)?;
            display_depth(f, depth)?;
            write!(f, "}}")?;
        }
        AmlTerm::VarPackage(size, elements) => {
            write!(f, "VarPackage (")?;
            display_term_arg(size, f, depth)?;
            write!(f, ") {{")?;
            display_terms_list(elements.iter(), Some(4), f, depth)?;
            writeln!(f)?;
            display_depth(f, depth)?;
            write!(f, "}}")?;
        }
        AmlTerm::Processor(processor) => {
            writeln!(
                f,
                "Processor ({}, 0x{:02X}, 0x{:04X}, 0x{:02X}) {{",
                processor.name, processor.unk1, processor.unk2, processor.unk3
            )?;
            display_terms(&processor.term_list, f, depth + 1)?;
            display_depth(f, depth)?;
            writeln!(f, "}}")?;
        }
        AmlTerm::PowerResource(power_resource) => {
            writeln!(
                f,
                "PowerResource ({}, 0x{:02X}, 0x{:04X}) {{",
                power_resource.name, power_resource.system_level, power_resource.resource_order,
            )?;
            display_terms(&power_resource.term_list, f, depth + 1)?;
            display_depth(f, depth)?;
            writeln!(f, "}}")?;
        }
        AmlTerm::String(str) => {
            write!(f, "\"{}\"", str.replace('\n', "\\n"))?;
        }
        AmlTerm::Method(method) => {
            display_method(method, f, depth)?;
        }
        AmlTerm::NameObj(name, term) => {
            write!(f, "Name({}, ", name)?;
            display_term_arg(term, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::ToHexString(term, target) => {
            display_call_term_target("ToHexString", &[term], &[target], f, depth)?;
        }
        AmlTerm::ToDecimalString(term, target) => {
            display_call_term_target("ToDecimalString", &[term], &[target], f, depth)?;
        }
        AmlTerm::ToInteger(term, target) => {
            display_call_term_target("ToInteger", &[term], &[target], f, depth)?;
        }
        AmlTerm::Mid(source_term, index_term, length_term, target) => {
            display_call_term_target(
                "Mid",
                &[source_term, index_term, length_term],
                &[target],
                f,
                depth,
            )?;
        }
        AmlTerm::ToBuffer(term, target) => {
            display_call_term_target("ToBuffer", &[term], &[target], f, depth)?;
        }
        AmlTerm::Store(arg, target) => {
            display_target(target, f, depth)?;
            write!(f, " = ")?;
            display_term_arg(arg, f, depth)?;
        }
        AmlTerm::SizeOf(target) => {
            display_call_term_target("SizeOf", &[], &[target], f, depth)?;
        }
        AmlTerm::Subtract(arg1, arg2, target) => {
            display_binary_op("-", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Add(arg1, arg2, target) => {
            display_binary_op("+", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Multiply(arg1, arg2, target) => {
            display_binary_op("*", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::ShiftLeft(arg1, arg2, target) => {
            display_binary_op("<<", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::ShiftRight(arg1, arg2, target) => {
            display_binary_op(">>", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Divide(term1, term2, target1, target2) => {
            display_binary_op("/", term1, term2, target2, f, depth)?;
            if !matches!(target1.as_ref(), Target::None) {
                write!(f, ", Reminder=")?;
                display_target(target1, f, depth)?;
            }
        }
        AmlTerm::Mod(arg1, arg2, target) => {
            display_binary_op("%", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::And(arg1, arg2, target) => {
            display_binary_op("&", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Nand(arg1, arg2, target) => {
            display_binary_op("~&", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Or(arg1, arg2, target) => {
            display_binary_op("|", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Nor(arg1, arg2, target) => {
            display_binary_op("~|", arg1, arg2, target, f, depth)?;
        }
        AmlTerm::Xor(arg1, arg2, target) => {
            display_binary_op("^", arg1, arg2, target, f, depth)?;
        }

        AmlTerm::LLess(arg1, arg2) => {
            display_binary_op("<", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LLessEqual(arg1, arg2) => {
            display_binary_op("<=", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LGreater(arg1, arg2) => {
            display_binary_op(">", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LGreaterEqual(arg1, arg2) => {
            display_binary_op(">=", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LEqual(arg1, arg2) => {
            display_binary_op("==", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LNotEqual(arg1, arg2) => {
            display_binary_op("!=", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LAnd(arg1, arg2) => {
            display_binary_op("&&", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LOr(arg1, arg2) => {
            display_binary_op("||", arg1, arg2, &Target::None, f, depth)?;
        }
        AmlTerm::LNot(arg) => {
            write!(f, "!")?;
            display_term_arg(arg, f, depth)?;
        }
        AmlTerm::Increment(target) => {
            display_target(target, f, depth)?;
            write!(f, "++")?;
        }
        AmlTerm::Decrement(target) => {
            display_target(target, f, depth)?;
            write!(f, "--")?;
        }

        AmlTerm::While(predicate_block) => {
            display_predicate_block("While", predicate_block, f, depth)?;
        }
        AmlTerm::If(predicate_block) => {
            display_predicate_block("If", predicate_block, f, depth)?;
        }
        AmlTerm::Else(term_list) => {
            writeln!(f, "Else {{")?;
            display_terms(term_list, f, depth + 1)?;
            display_depth(f, depth)?;
            write!(f, "}}")?;
        }
        AmlTerm::Break => {
            write!(f, "Break")?;
        }
        AmlTerm::Return(term) => {
            write!(f, "Return ")?;
            display_term_arg(term, f, depth)?;
        }
        AmlTerm::DerefOf(term) => {
            display_call_term_target("DerefOf", &[term], &[], f, depth)?;
        }
        AmlTerm::RefOf(target) => {
            display_call_term_target("RefOf", &[], &[target], f, depth)?;
        }
        AmlTerm::Index(term1, term2, target) => {
            display_index(term1, term2, target, f, depth)?;
        }
        AmlTerm::Buffer(size, data) => {
            write!(f, "Buffer (")?;
            display_term_arg(size, f, depth)?;
            write!(f, ") {{")?;
            for (i, byte) in data.iter().enumerate() {
                if i % 16 == 0 {
                    writeln!(f)?;
                    display_depth(f, depth + 1)?;
                }
                write!(f, "0x{:02X} ", byte)?;
            }
            writeln!(f)?;
            display_depth(f, depth)?;
            write!(f, "}}")?;
        }
        AmlTerm::Mutex(name, sync_level) => {
            write!(f, "Mutex ({}, {})", name, sync_level)?;
        }
        AmlTerm::Event(name) => {
            write!(f, "Event ({})", name)?;
        }
        AmlTerm::CondRefOf(target1, target2) => {
            display_call_term_target("CondRefOf", &[], &[target1, target2], f, depth)?;
        }
        AmlTerm::CreateFieldOp(source, index, numbits, target_name) => {
            display_call_term_target(
                "CreateField",
                &[source, index, numbits],
                &[&Target::Name(target_name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::Stall(term) => {
            display_call_term_target("Stall", &[term], &[], f, depth)?;
        }
        AmlTerm::Sleep(term) => {
            display_call_term_target("Sleep", &[term], &[], f, depth)?;
        }
        AmlTerm::Acquire(target, timeout) => {
            write!(f, "Acquire (")?;
            display_target(target, f, depth)?;
            write!(f, ", 0x{timeout:04X})")?;
        }
        AmlTerm::Signal(target) => {
            display_call_term_target("Signal", &[], &[target], f, depth)?;
        }
        AmlTerm::Wait(target, timeout) => {
            write!(f, "Wait (")?;
            display_target(target, f, depth)?;
            write!(f, ", ")?;
            display_term_arg(timeout, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::Reset(target) => {
            display_call_term_target("Reset", &[], &[target], f, depth)?;
        }
        AmlTerm::Release(target) => {
            display_call_term_target("Release", &[], &[target], f, depth)?;
        }
        AmlTerm::Notify(target, value) => {
            write!(f, "Notify (")?;
            display_target(target, f, depth)?;
            write!(f, ", ")?;
            display_term_arg(value, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::CreateBitField(term1, term2, name) => {
            display_call_term_target(
                "CreateBitField",
                &[term1, term2],
                &[&Target::Name(name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::CreateByteField(term1, term2, name) => {
            display_call_term_target(
                "CreateByteField",
                &[term1, term2],
                &[&Target::Name(name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::CreateWordField(term1, term2, name) => {
            display_call_term_target(
                "CreateWordField",
                &[term1, term2],
                &[&Target::Name(name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::CreateDWordField(term1, term2, name) => {
            display_call_term_target(
                "CreateDWordField",
                &[term1, term2],
                &[&Target::Name(name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::CreateQWordField(term1, term2, name) => {
            display_call_term_target(
                "CreateQWordField",
                &[term1, term2],
                &[&Target::Name(name.clone())],
                f,
                depth,
            )?;
        }
        AmlTerm::MethodCall(name, args) => {
            write!(f, "{} (", name)?;
            display_terms_list(args.iter(), None, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::Concat(term1, term2, target) => {
            display_call_term_target("Concat", &[term1, term2], &[target], f, depth)?;
        }
        AmlTerm::Not(term, target) => {
            display_call_term_target("Not", &[term], &[target], f, depth)?;
        }
        AmlTerm::FindSetLeftBit(term, target) => {
            display_call_term_target("FindSetLeftBit", &[term], &[target], f, depth)?;
        }
        AmlTerm::FindSetRightBit(term, target) => {
            display_call_term_target("FindSetRightBit", &[term], &[target], f, depth)?;
        }
        AmlTerm::ConcatRes(term1, term2, target) => {
            display_call_term_target("ConcatRes", &[term1, term2], &[target], f, depth)?;
        }
        AmlTerm::ObjectType(obj) => {
            write!(f, "ObjectType (")?;
            display_target(obj, f, depth)?;
            write!(f, ")")?;
        }
        AmlTerm::Noop => {
            write!(f, "Noop")?;
        }
    }
    Ok(())
}

impl fmt::Display for AmlCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_terms(&self.term_list, f, 0)
    }
}

impl AmlCode {
    #[allow(dead_code)]
    pub fn display_with_depth(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        display_terms(&self.term_list, f, depth)
    }
}
