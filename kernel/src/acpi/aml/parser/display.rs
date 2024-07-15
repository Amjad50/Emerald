use core::fmt;

use crate::acpi::aml::display::{AmlDisplayer, HexHolder};

use super::{
    AccessAttrib, AccessType, AmlCode, AmlTerm, Buffer, FieldConnection, FieldDef, FieldElement,
    FieldUpdateRule, IndexFieldDef, IntegerData, MethodObj, PackageElement, PowerResource,
    PredicateBlock, ProcessorDeprecated, RegionObj, RegionSpace, ScopeObj, ScopeType, Target,
    TermArg, UnresolvedDataObject,
};

fn display_target_assign<F>(
    f: &mut fmt::Formatter<'_>,
    target: &Target,
    value_fmt: F,
) -> fmt::Result
where
    F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    if !matches!(target, Target::None) {
        fmt::Display::fmt(target, f)?;
        f.write_str(" = ")?;
    }

    value_fmt(f)
}

impl fmt::Display for RegionSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(x) => write!(f, "0x{:02X}", x),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl fmt::Display for RegionObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        AmlDisplayer::start(f, "OperationRegion")
            .paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "{:?}", self.region_space))
            .paren_arg(|f| write!(f, "{}", self.region_offset))
            .paren_arg(|f| write!(f, "{}", self.region_length))
            .finish()
    }
}

impl fmt::Display for FieldDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "Field");
        d.paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "{}", self.access_type))
            .paren_arg(|f| write!(f, "{}", if self.need_lock { "Lock" } else { "NoLock" }))
            .paren_arg(|f| write!(f, "{}", self.update_rule))
            .finish_paren_arg()
            .set_list(true);

        for field in &self.fields {
            d.body_field(|f| field.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for IndexFieldDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "IndexField");
        d.paren_arg(|f| f.write_str(&self.index_name))
            .paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "{}", self.access_type))
            .paren_arg(|f| write!(f, "{}", if self.need_lock { "Lock" } else { "NoLock" }))
            .paren_arg(|f| write!(f, "{}", self.update_rule))
            .finish_paren_arg()
            .set_list(true);

        for field in &self.fields {
            d.body_field(|f| field.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for ScopeObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ty = match self.ty {
            ScopeType::Device => "Device",
            ScopeType::Scope => "Scope",
        };
        let mut d = AmlDisplayer::start(f, ty);
        d.paren_arg(|f| f.write_str(&self.name)).finish_paren_arg();
        for term in &self.term_list {
            d.body_field(|f| term.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for MethodObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "Method");
        d.paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "{}", self.num_args))
            .paren_arg(|f| {
                f.write_str(if self.is_serialized {
                    "Serialized"
                } else {
                    "NotSerialized"
                })
            });
        if self.sync_level != 0 {
            d.paren_arg(|f: &mut fmt::Formatter| write!(f, "0x{:X}", self.sync_level));
        }

        d.finish_paren_arg();

        for term in &self.term_list {
            d.body_field(|f| term.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for ProcessorDeprecated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "Processor");
        d.paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "0x{:02X}", self.unk1))
            .paren_arg(|f| write!(f, "0x{:08X}", self.unk2))
            .paren_arg(|f| write!(f, "0x{:02X}", self.unk3))
            .finish_paren_arg();

        for term in &self.term_list {
            d.body_field(|f| term.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for PowerResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "PowerResource");
        d.paren_arg(|f| f.write_str(&self.name))
            .paren_arg(|f| write!(f, "0x{:02X}", self.system_level))
            .paren_arg(|f| write!(f, "0x{:04X}", self.resource_order))
            .finish_paren_arg();

        for term in &self.term_list {
            d.body_field(|f| term.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for PredicateBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "");
        d.paren_arg(|f| self.predicate.fmt(f));

        for term in &self.term_list {
            d.body_field(|f| term.fmt(f));
        }

        d.finish()
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "Buffer");
        d.paren_arg(|f| self.size.fmt(f))
            .finish_paren_arg()
            .set_list(true);

        for element in self.data.iter() {
            d.body_field(|f| write!(f, "0x{:02X}", element));
        }

        d.finish()
    }
}

impl fmt::Display for AccessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)?;
        f.write_str("Acc")
    }
}

impl fmt::Display for AccessAttrib {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessAttrib::ByteValue(v) => write!(f, "0x{:02X}", v),
            AccessAttrib::Bytes(v) => write!(f, "AttribBytes ({})", v),
            AccessAttrib::RawBytes(v) => write!(f, "AttribRawBytes ({})", v),
            AccessAttrib::RawProcessBytes(v) => write!(f, "AttribRawProcessBytes ({})", v),
            AccessAttrib::Quick => f.write_str("AttribQuick"),
            AccessAttrib::SendRecv => f.write_str("AttribSendReceive"),
            AccessAttrib::Byte => f.write_str("AttribByte"),
            AccessAttrib::Word => f.write_str("AttribWord"),
            AccessAttrib::Block => f.write_str("AttribBlock"),
            AccessAttrib::ProcessCall => f.write_str("AttribProcessCall"),
            AccessAttrib::BlockProcessCall => f.write_str("AttribBlockProcessCall"),
        }
    }
}

impl fmt::Display for FieldUpdateRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for FieldConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldConnection::Buffer(buffer) => buffer.fmt(f),
            FieldConnection::Name(name) => f.write_str(name),
        }
    }
}

impl fmt::Display for FieldElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldElement::Offset(offset) => write!(f, "Offset (0x{:02X})", offset),
            FieldElement::Named(name, len) => {
                write!(
                    f,
                    "{}, {}{}",
                    name,
                    if f.alternate() { "  " } else { "" },
                    len
                )
            }
            FieldElement::Access(access_type, access_attrib) => write!(
                f,
                "AccessAs {}({access_type}, {access_attrib})",
                if f.alternate() { " " } else { "" },
            ),
            FieldElement::Connection(connection) => write!(f, "{connection}"),
        }
    }
}

impl fmt::Display for TermArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TermArg::Expression(term) => term.fmt(f),
            TermArg::DataObject(dataobj) => dataobj.fmt(f),
            TermArg::Arg(arg) => write!(f, "Arg{:x}", arg),
            TermArg::Local(local) => write!(f, "Local{:x}", local),
            TermArg::Name(name) => f.write_str(name),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::None => write!(f, "None"),
            Target::Arg(arg) => write!(f, "Arg{:x}", arg),
            Target::Local(local) => write!(f, "Local{:x}", local),
            Target::Name(name) => f.write_str(name),
            Target::Debug => f.write_str("Debug"),
            Target::DerefOf(term_arg) => AmlDisplayer::start(f, "DerefOf")
                .paren_arg(|f| term_arg.fmt(f))
                .finish(),
            Target::RefOf(target) => AmlDisplayer::start(f, "RefOf")
                .paren_arg(|f| target.fmt(f))
                .finish(),
            Target::Index(term_arg1, term_arg2, target) => {
                display_target_assign(f, target.as_ref(), |f| {
                    term_arg1.fmt(f)?;
                    f.write_str("[")?;
                    term_arg2.fmt(f)?;
                    f.write_str("]")
                })
            }
        }
    }
}

impl fmt::Display for UnresolvedDataObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnresolvedDataObject::Integer(int) => int.fmt(f),
            UnresolvedDataObject::Buffer(buffer) => buffer.fmt(f),
            UnresolvedDataObject::Package(size, elements) => {
                let mut d = AmlDisplayer::start(f, "Package");
                d.paren_arg(|f| write!(f, "0x{:X}", size))
                    .finish_paren_arg()
                    .set_list(true);

                for element in elements {
                    d.body_field(|f| element.fmt(f));
                }

                d.finish()
            }
            UnresolvedDataObject::VarPackage(size, elements) => {
                let mut d = AmlDisplayer::start(f, "Package");
                d.paren_arg(|f| size.fmt(f))
                    .finish_paren_arg()
                    .set_list(true);

                for element in elements {
                    d.body_field(|f| element.fmt(f));
                }

                d.finish()
            }
            UnresolvedDataObject::String(str) => {
                write!(f, "\"{}\"", str.replace('\n', "\\n"))
            }
            UnresolvedDataObject::EisaId(eisa_id) => write!(f, "EisaId ({:?})", eisa_id),
        }
    }
}

impl fmt::Display for IntegerData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntegerData::ConstZero => write!(f, "Zero"),
            IntegerData::ConstOne => write!(f, "One"),
            IntegerData::ConstOnes => write!(f, "0xFFFFFFFFFFFFFFFF"),
            IntegerData::ByteConst(data) => write!(f, "0x{:02X}", data),
            IntegerData::WordConst(data) => write!(f, "0x{:04X}", data),
            IntegerData::DWordConst(data) => write!(f, "0x{:08X}", data),
            IntegerData::QWordConst(data) => write!(f, "0x{:016X}", data),
        }
    }
}

impl<D: fmt::Display> fmt::Display for PackageElement<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageElement::DataObject(dataobj) => dataobj.fmt(f),
            PackageElement::Name(name) => f.write_str(name),
        }
    }
}

impl fmt::Display for AmlTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn display_func_like(
            f: &mut fmt::Formatter<'_>,
            name: &str,
            elements: &[&dyn fmt::Display],
        ) -> fmt::Result {
            let mut d = AmlDisplayer::start(f, name);

            for element in elements {
                d.paren_arg(|f| element.fmt(f));
            }

            d.finish()
        }

        fn display_binary_op(
            f: &mut fmt::Formatter<'_>,
            op: &str,
            left: &dyn fmt::Display,
            right: &dyn fmt::Display,
        ) -> fmt::Result {
            f.write_str("( ")?;
            left.fmt(f)?;
            f.write_str(" ")?;
            f.write_str(op)?;
            f.write_str(" ")?;
            right.fmt(f)?;
            f.write_str(" )")
        }

        match self {
            AmlTerm::Scope(scope) => scope.fmt(f),
            AmlTerm::Region(region) => region.fmt(f),
            AmlTerm::Field(field) => field.fmt(f),
            AmlTerm::IndexField(field) => field.fmt(f),
            AmlTerm::Device(scope) => scope.fmt(f),
            AmlTerm::Processor(processor) => processor.fmt(f),
            AmlTerm::PowerResource(resource) => resource.fmt(f),
            AmlTerm::Method(method) => method.fmt(f),
            AmlTerm::NameObj(name, object) => AmlDisplayer::start(f, "Name")
                .paren_arg(|f| write!(f, "{}", name))
                .paren_arg(|f| object.fmt(f))
                .finish(),
            AmlTerm::Alias(source, alias) => display_func_like(f, "Alias", &[source, alias]),
            AmlTerm::ToHexString(term_arg, target) => {
                display_func_like(f, "ToHexString", &[term_arg, target])
            }
            AmlTerm::ToBuffer(term_arg, target) => {
                display_func_like(f, "ToBuffer", &[term_arg, target])
            }
            AmlTerm::ToDecimalString(term_arg, target) => {
                display_func_like(f, "ToDecimalString", &[term_arg, target])
            }
            AmlTerm::ToInteger(term_arg, target) => {
                display_func_like(f, "ToInteger", &[term_arg, target])
            }
            AmlTerm::Mid(term1, term2, term3, target) => {
                display_func_like(f, "Mid", &[term1, term2, term3, target])
            }
            AmlTerm::Add(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "+", term1, term2))
            }
            AmlTerm::Concat(term1, term2, target) => {
                display_func_like(f, "Concatenate", &[term1, term2, target])
            }
            AmlTerm::Subtract(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "-", term1, term2))
            }
            AmlTerm::Multiply(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "*", term1, term2))
            }
            AmlTerm::Divide(term1, term2, reminder, target) => {
                if matches!(reminder.as_ref(), Target::None) {
                    display_target_assign(f, target, |f| display_binary_op(f, "/", term1, term2))
                } else {
                    display_func_like(f, "Divide", &[term1, term2, reminder, target])
                }
            }
            AmlTerm::ShiftLeft(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "<<", term1, term2))
            }
            AmlTerm::ShiftRight(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, ">>", term1, term2))
            }
            AmlTerm::And(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "&", term1, term2))
            }
            AmlTerm::Nand(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "~&", term1, term2))
            }
            AmlTerm::Or(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "|", term1, term2))
            }
            AmlTerm::Nor(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "~|", term1, term2))
            }
            AmlTerm::Xor(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "^", term1, term2))
            }
            AmlTerm::Not(term, target) => {
                display_target_assign(f, target, |f| write!(f, "~{}", term))
            }
            AmlTerm::SizeOf(target) => display_func_like(f, "SizeOf", &[target]),
            AmlTerm::Store(term_arg, target) => {
                target.fmt(f)?;
                f.write_str(" = ")?;
                term_arg.fmt(f)
            }
            AmlTerm::RefOf(obj) => display_func_like(f, "RefOf", &[obj]),
            AmlTerm::Increment(target) => {
                target.fmt(f)?;
                f.write_str("++")
            }
            AmlTerm::Decrement(target) => {
                target.fmt(f)?;
                f.write_str("--")
            }
            AmlTerm::While(predicate_block) => {
                f.write_str("While ")?;
                predicate_block.fmt(f)
            }
            AmlTerm::If(predicate_block) => {
                f.write_str("If ")?;
                predicate_block.fmt(f)
            }
            AmlTerm::Else(terms) => {
                f.write_str("Else ")?;
                let mut d = AmlDisplayer::start(f, "");
                for term in terms {
                    d.body_field(|f| term.fmt(f));
                }

                d.finish()
            }
            AmlTerm::Noop => f.write_str("Noop"),
            AmlTerm::Return(term) => {
                f.write_str("Return ( ")?;
                term.fmt(f)?;
                f.write_str(" )")
            }
            AmlTerm::Break => f.write_str("Break"),
            AmlTerm::LAnd(term1, term2) => display_binary_op(f, "&&", term1, term2),
            AmlTerm::LOr(term1, term2) => display_binary_op(f, "||", term1, term2),
            AmlTerm::LNot(term) => {
                write!(f, "!{}", term)
            }
            AmlTerm::LNotEqual(term1, term2) => display_binary_op(f, "!=", term1, term2),
            AmlTerm::LLessEqual(term1, term2) => display_binary_op(f, "<=", term1, term2),
            AmlTerm::LGreaterEqual(term1, term2) => display_binary_op(f, ">=", term1, term2),
            AmlTerm::LEqual(term1, term2) => display_binary_op(f, "==", term1, term2),
            AmlTerm::LGreater(term1, term2) => display_binary_op(f, ">", term1, term2),
            AmlTerm::LLess(term1, term2) => display_binary_op(f, "<", term1, term2),
            AmlTerm::FindSetLeftBit(term, target) => {
                display_func_like(f, "FindSetLeftBit", &[term, target])
            }
            AmlTerm::FindSetRightBit(term, target) => {
                display_func_like(f, "FindSetRightBit", &[term, target])
            }
            AmlTerm::DerefOf(term_arg) => display_func_like(f, "DerefOf", &[term_arg]),
            AmlTerm::ConcatRes(term1, term2, target) => {
                display_func_like(f, "ConcatenateResTemplate", &[term1, term2, target])
            }
            AmlTerm::Mod(term1, term2, target) => {
                display_target_assign(f, target, |f| display_binary_op(f, "%", term1, term2))
            }
            AmlTerm::Notify(target, term_arg) => {
                display_func_like(f, "Notify", &[target, term_arg])
            }
            AmlTerm::Index(term1, term2, target) => display_target_assign(f, target, |f| {
                term1.fmt(f)?;
                write!(f, "[")?;
                term2.fmt(f)?;
                write!(f, "]")
            }),
            AmlTerm::Mutex(name, num) => {
                write!(f, "Mutex({name}, 0x{num:02X})")
            }
            AmlTerm::Event(name) => display_func_like(f, "Event", &[name]),
            AmlTerm::CondRefOf(src, dest) => display_func_like(f, "CondRefOf", &[src, dest]),
            AmlTerm::CreateFieldOp(src, bit_index, num_bits, field_name) => {
                display_func_like(f, "CreateFieldOp", &[src, bit_index, num_bits, field_name])
            }
            AmlTerm::Acquire(target, num) => {
                // TODO: display number in hex, using maybe custom struct
                display_func_like(f, "Acquire", &[target, &HexHolder(num)])
            }
            AmlTerm::Signal(target) => display_func_like(f, "Signal", &[target]),
            AmlTerm::Wait(target, term) => display_func_like(f, "Wait", &[target, term]),
            AmlTerm::Reset(target) => display_func_like(f, "Reset", &[target]),
            AmlTerm::Release(target) => display_func_like(f, "Release", &[target]),
            AmlTerm::Stall(term_arg) => display_func_like(f, "Stall", &[term_arg]),
            AmlTerm::Sleep(term_arg) => display_func_like(f, "Sleep", &[term_arg]),
            AmlTerm::CreateDWordField(term1, term2, name) => {
                display_func_like(f, "CreateDWordField", &[term1, term2, name])
            }
            AmlTerm::CreateWordField(term1, term2, name) => {
                display_func_like(f, "CreateWordField", &[term1, term2, name])
            }
            AmlTerm::CreateByteField(term1, term2, name) => {
                display_func_like(f, "CreateByteField", &[term1, term2, name])
            }
            AmlTerm::CreateBitField(term1, term2, name) => {
                display_func_like(f, "CreateBitField", &[term1, term2, name])
            }
            AmlTerm::CreateQWordField(term1, term2, name) => {
                display_func_like(f, "CreateQWordField", &[term1, term2, name])
            }
            AmlTerm::MethodCall(name, args) => {
                let mut d = AmlDisplayer::start(f, name);

                for arg in args {
                    d.paren_arg(|f| arg.fmt(f));
                }

                d.finish()
            }
            AmlTerm::ObjectType(obj) => display_func_like(f, "ObjectType", &[obj]),
        }
    }
}

impl fmt::Display for AmlCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, term) in self.term_list.iter().enumerate() {
            term.fmt(f)?;

            if i < self.term_list.len() - 1 {
                if f.alternate() {
                    writeln!(f)?;
                } else {
                    write!(f, "; ")?;
                }
            }
        }
        Ok(())
    }
}
