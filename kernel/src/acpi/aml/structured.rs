use core::fmt;

use alloc::{
    collections::{btree_map::Entry, BTreeMap},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use tracing::warn;

use crate::testing;

use super::{
    display::AmlDisplayer,
    parser::{
        AmlTerm, FieldDef, IndexFieldDef, MethodObj, PowerResource, ProcessorDeprecated, RegionObj,
        ScopeType, UnresolvedDataObject,
    },
    AmlCode,
};

#[derive(Debug, Clone)]
pub enum StructuredAmlError {
    QueryPathMustBeAbsolute,
    /// This is a very stupid error, its a bit annoying to return `\\` scope element, since its not
    /// stored inside an `Element`, and we can't return a temporary value
    /// We will never (normally) execute or try to find the label `\\`, so hopefully we can rely on that XD
    /// FIXME: get a better fix
    CannotQueryRoot,
    PartOfPathNotScope(String),
    InvalidName(String),
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StructuredAml {
    /// Denoted by `\`
    root: Scope,
}

impl StructuredAml {
    pub fn parse(code: &AmlCode) -> Self {
        // We use `root` as a root reference, and `root_terms` that will be the scope
        // of the root `code`, the reason we have two is that some statement use `\_SB` for example
        // and some will just use `_SB` in the root, those are the same thing.
        let mut root = Scope::default();
        let root_terms = Scope::parse(&code.term_list, &mut root, "\\", ScopeType::Scope);

        root.merge(root_terms);

        Self { root }
    }

    pub fn find_object(&self, label: &str) -> Result<Option<&ElementType>, StructuredAmlError> {
        if let Some(rest) = label.strip_prefix('\\') {
            if rest.is_empty() {
                // sad
                return Err(StructuredAmlError::CannotQueryRoot);
            }
            self.root.find_object(rest)
        } else {
            Err(StructuredAmlError::QueryPathMustBeAbsolute)
        }
    }
}

#[derive(Debug, Clone)]
pub enum ElementType {
    ScopeOrDevice(Scope),
    Method(MethodObj),
    Processor(ProcessorDeprecated),
    PowerResource(PowerResource),
    RegionFields(Option<RegionObj>, Vec<FieldDef>),
    IndexField(IndexFieldDef),
    Name(UnresolvedDataObject),
    UnknownElements(Vec<AmlTerm>),
}

#[derive(Debug, Clone)]
pub struct Scope {
    ty: ScopeType,
    children: BTreeMap<String, ElementType>,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            ty: ScopeType::Scope,
            children: Default::default(),
        }
    }
}

impl Scope {
    pub fn parse(terms: &[AmlTerm], root: &mut Scope, current_path: &str, ty: ScopeType) -> Self {
        let mut this = Scope {
            ty,
            children: Default::default(),
        };

        fn handle_add(
            current_path: &str,
            this: &mut Scope,
            root: &mut Scope,
            name: &str,
            element: ElementType,
        ) {
            if let Some(rest) = name.strip_prefix('\\') {
                if rest.is_empty() {
                    if let ElementType::ScopeOrDevice(scope) = element {
                        root.merge(scope);
                    } else {
                        panic!("Root is not a scope or device");
                    }
                } else {
                    root.add_child(rest, element);
                }
            } else if let Some(rest) = name.strip_prefix('^') {
                assert!(!rest.is_empty());
                // this will add to the parent
                let (parent, _) = current_path.rsplit_once('.').expect("Must have parent");
                assert!(
                    parent.starts_with('\\'),
                    "parent {parent:?} must start with \\"
                );

                if rest.starts_with('^') {
                    // recurse
                    handle_add(parent, this, root, rest, element);
                } else {
                    let full_path = format!("{}.{}", parent, rest);
                    root.add_child(full_path.trim_start_matches('\\'), element);
                }
            } else {
                this.add_child(name, element);
            }
        }

        for term in terms {
            match term {
                AmlTerm::Device(scope) | AmlTerm::Scope(scope) => {
                    let scope_path = if scope.name.starts_with('\\') {
                        scope.name.clone()
                    } else {
                        format!(
                            "{}{}{}",
                            current_path,
                            if current_path.ends_with('\\') {
                                ""
                            } else {
                                "."
                            },
                            scope.name
                        )
                    };
                    let element = ElementType::ScopeOrDevice(Scope::parse(
                        &scope.term_list,
                        root,
                        &scope_path,
                        match term {
                            AmlTerm::Device(_) => ScopeType::Device,
                            AmlTerm::Scope(_) => ScopeType::Scope,
                            _ => unreachable!(),
                        },
                    ));
                    handle_add(current_path, &mut this, root, &scope.name, element);
                }
                AmlTerm::Region(region) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &region.name,
                        ElementType::RegionFields(Some(region.clone()), Vec::new()),
                    );
                }
                AmlTerm::Field(field) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &field.name,
                        ElementType::RegionFields(None, vec![field.clone()]),
                    );
                }
                AmlTerm::IndexField(index_field) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &index_field.name,
                        ElementType::IndexField(index_field.clone()),
                    );
                }
                AmlTerm::Processor(processor) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &processor.name,
                        ElementType::Processor(processor.clone()),
                    );
                }
                AmlTerm::PowerResource(power_resource) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &power_resource.name,
                        ElementType::PowerResource(power_resource.clone()),
                    );
                }
                AmlTerm::Method(method) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        &method.name,
                        ElementType::Method(method.clone()),
                    );
                }
                AmlTerm::NameObj(name, obj) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        name,
                        ElementType::Name(obj.clone()),
                    );
                }
                _ => {
                    // TODO: the current way to structure is not good
                    //       since the root may contain execution elements, that we need to take care of
                    //       the language works similar to python in some sense. for example
                    //       ```
                    //       If (( PWRS & 0x02 )) {
                    //         Name(_S1_, Package (0x02) {
                    //           One, One
                    //         })
                    //       }
                    //       ```
                    //       this will enable `\_S1` name based on `PWRS` flags, and this is in the root scope
                    //       so better to rewrite this whole thing :(
                    warn!("Execution statements found in scope {term:?}");
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        "\\UNKW",
                        ElementType::UnknownElements(vec![term.clone()]),
                    );
                }
            }
        }

        this
    }

    // specific version for fast addition than `add_child`
    fn add_immediate_child(&mut self, name: &str, element: ElementType) {
        assert!(!name.starts_with('\\'));
        assert_eq!(name.len(), 4, "Invalid name: {name:?}");
        match self.children.entry(name.to_string()) {
            Entry::Vacant(entry) => {
                entry.insert(element);
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                ElementType::ScopeOrDevice(scope) => {
                    let ElementType::ScopeOrDevice(mut element) = element else {
                        panic!("New element: {name:?} is not a scope or device");
                    };

                    // device always wins if there is any, its a more special version of `Scope`
                    // it shouldn't conflict anyway, but just in case
                    if matches!(scope.ty, ScopeType::Device)
                        || matches!(element.ty, ScopeType::Device)
                    {
                        element.ty = ScopeType::Device;
                    }

                    scope.merge(element);
                }
                ElementType::RegionFields(region, fields) => {
                    let ElementType::RegionFields(new_region, new_fields) = element else {
                        panic!("New element: {name:?} is not a region");
                    };

                    assert!(
                        !(region.is_some() && new_region.is_some()),
                        "Both regions are available, conflict, {region:?} && {new_region:?}"
                    );
                    *region = region.clone().or(new_region);
                    fields.extend(new_fields);
                }
                ElementType::UnknownElements(elements) => {
                    let ElementType::UnknownElements(new_elements) = element else {
                        panic!("New element: {name:?} is not an unknown element");
                    };
                    elements.extend(new_elements);
                }
                _ => panic!("Child: {name:?} is not a scope or device"),
            },
        }
    }

    pub fn add_child(&mut self, name: &str, mut element: ElementType) {
        assert!(!name.starts_with('\\'));
        let split_result = name.split_once('.');

        // change the name
        match element {
            ElementType::Method(ref mut method) => {
                method.name = name.to_string();
            }
            ElementType::Processor(ref mut processor) => {
                processor.name = name.to_string();
            }
            ElementType::PowerResource(ref mut power_resource) => {
                power_resource.name = name.to_string();
            }
            ElementType::RegionFields(ref mut region, ref mut fields) => {
                if let Some(r) = region.as_mut() {
                    r.name = name.to_string();
                }
                for field in fields {
                    field.name = name.to_string();
                }
            }
            ElementType::IndexField(ref mut index_field) => {
                index_field.name = name.to_string();
            }
            _ => {}
        }

        match split_result {
            Some((first_child, rest)) => {
                let child = self
                    .children
                    .entry(first_child.to_string())
                    .or_insert(ElementType::ScopeOrDevice(Scope::default()));

                if let ElementType::ScopeOrDevice(scope) = child {
                    scope.add_child(rest, element);
                } else {
                    panic!(
                        "Child: {first_child:?} of  {name:?} is not a scope or device {:?}",
                        child
                    );
                }
            }
            None => {
                self.add_immediate_child(name, element);
            }
        }
    }

    pub fn merge(&mut self, other: Scope) {
        for (name, element) in other.children {
            self.add_immediate_child(&name, element)
        }
    }

    fn find_object(&self, name: &str) -> Result<Option<&ElementType>, StructuredAmlError> {
        let split_result = name.split_once('.');

        match split_result {
            Some((first_child, rest)) => {
                if first_child.len() != 4 {
                    return Err(StructuredAmlError::InvalidName(first_child.to_string()));
                }
                let Some(child) = self.children.get(first_child) else {
                    return Ok(None);
                };

                if let ElementType::ScopeOrDevice(scope) = child {
                    scope.find_object(rest)
                } else {
                    Err(StructuredAmlError::PartOfPathNotScope(
                        first_child.to_string(),
                    ))
                }
            }
            None => {
                if name.len() != 4 {
                    return Err(StructuredAmlError::InvalidName(name.to_string()));
                }
                Ok(self.children.get(name))
            }
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (name, element)) in self.children.iter().enumerate() {
            match element {
                ElementType::ScopeOrDevice(scope) => {
                    let ty = match self.ty {
                        ScopeType::Scope => "Scope",
                        ScopeType::Device => "Device",
                    };
                    let mut d = AmlDisplayer::start(f, ty);
                    d.paren_arg(|f| f.write_str(name)).finish_paren_arg();

                    d.body_field(|f| scope.fmt(f));

                    d.at_least_empty_body().finish()
                }
                ElementType::Method(method) => method.fmt(f),
                ElementType::Processor(processor) => processor.fmt(f),
                ElementType::PowerResource(power_resource) => power_resource.fmt(f),
                ElementType::RegionFields(region, fields) => {
                    if let Some(region) = region {
                        region.fmt(f)?;
                    } else {
                        write!(f, "Region {name}, NOT FOUND!!!")?;
                    }

                    if f.alternate() {
                        writeln!(f)?;
                    } else {
                        write!(f, "; ")?;
                    }

                    for (i, field) in fields.iter().enumerate() {
                        field.fmt(f)?;

                        if i < fields.len() - 1 {
                            if f.alternate() {
                                writeln!(f)?;
                            } else {
                                write!(f, "; ")?;
                            }
                        }
                    }

                    Ok(())
                }
                ElementType::IndexField(index_field) => index_field.fmt(f),
                ElementType::Name(data_obj) => AmlDisplayer::start(f, "Name")
                    .paren_arg(|f| f.write_str(name))
                    .paren_arg(|f| data_obj.fmt(f))
                    .finish(),
                ElementType::UnknownElements(elements) => {
                    let mut d = AmlDisplayer::start(f, "UnknownElements");

                    d.paren_arg(|f| f.write_str(name)).finish_paren_arg();

                    for element in elements {
                        d.body_field(|f| element.fmt(f));
                    }

                    d.at_least_empty_body().finish()
                }
            }?;

            if i < self.children.len() - 1 {
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

impl fmt::Display for StructuredAml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "Scope");
        d.paren_arg(|f| f.write_str("\\")).finish_paren_arg();

        d.body_field(|f| self.root.fmt(f));

        d.finish()
    }
}

testing::test! {
    fn test_structure() {
        use super::parser::{
            AccessType, FieldElement, FieldUpdateRule, IntegerData, ScopeObj, Target, TermArg,
            UnresolvedDataObject, RegionSpace
        };
        use alloc::boxed::Box;

        let code = AmlCode {
            term_list: vec![
                AmlTerm::Scope(ScopeObj {
                    ty: ScopeType::Scope,
                    name: "\\".to_string(),
                    term_list: vec![
                        AmlTerm::Region(RegionObj {
                            name: "DBG_".to_string(),
                            region_space: RegionSpace::SystemIO,
                            region_offset: TermArg::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::WordConst(1026),
                            )),
                            region_length: TermArg::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ConstOne,
                            )),
                        }),
                        AmlTerm::Field(FieldDef {
                            name: "DBG_".to_string(),
                            access_type: AccessType::Byte,
                            need_lock: false,
                            update_rule: FieldUpdateRule::Preserve,
                            fields: vec![FieldElement::Named("DBGB".to_string(), 8)],
                        }),
                        AmlTerm::Method(MethodObj {
                            name: "DBUG".to_string(),
                            num_args: 1,
                            is_serialized: false,
                            sync_level: 0,
                            term_list: vec![
                                AmlTerm::ToHexString(TermArg::Arg(0), Box::new(Target::Local(0))),
                                AmlTerm::ToBuffer(TermArg::Local(0), Box::new(Target::Local(0))),
                            ],
                        }),
                    ],
                }),
                AmlTerm::Method(MethodObj {
                    name: "\\_GPE._E02".to_string(),
                    num_args: 0,
                    is_serialized: false,
                    sync_level: 0,
                    term_list: vec![AmlTerm::MethodCall("\\_SB_.CPUS.CSCN".to_string(), vec![])],
                }),
            ],
        };

        let structured = StructuredAml::parse(&code);

        assert_eq!(
            structured.root.children.keys().collect::<Vec<_>>(),
            vec!["DBG_", "DBUG", "_GPE"]
        );

        match &structured.root.children["DBG_"] {
            ElementType::RegionFields(region, fields) => {
                assert!(region.is_some());
                assert!(!fields.is_empty());
            }
            _ => panic!("DBG_ is not a region"),
        }
        match &structured.root.children["DBUG"] {
            ElementType::Method(method) => {
                assert_eq!(method.name, "DBUG");
                assert_eq!(method.term_list.len(), 2);
            }
            _ => panic!("DBUG is not a method"),
        }
        match &structured.root.children["_GPE"] {
            ElementType::ScopeOrDevice(scope) => {
                assert_eq!(scope.children.keys().collect::<Vec<_>>(), vec!["_E02"]);

                match &scope.children["_E02"] {
                    ElementType::Method(method) => {
                        assert_eq!(method.name, "_E02");
                        assert_eq!(method.term_list.len(), 1);
                    }
                    _ => panic!("_E02 is not a method"),
                }
            }
            _ => panic!("_GPE is not a scope"),
        }
    }
}
