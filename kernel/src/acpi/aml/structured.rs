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
    parser::{
        self, AmlTerm, DataObject, FieldDef, IndexFieldDef, MethodObj, PowerResource,
        ProcessorDeprecated, RegionObj,
    },
    AmlCode,
};

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
        let root_terms = Scope::parse(&code.term_list, &mut root, "\\");

        root.merge(root_terms);

        Self { root }
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
    Name(DataObject),
    Mutex(u8),
    UnknownElements(Vec<AmlTerm>),
}

#[derive(Debug, Clone, Default)]
pub struct Scope {
    children: BTreeMap<String, ElementType>,
}

impl Scope {
    pub fn parse(terms: &[AmlTerm], root: &mut Scope, current_path: &str) -> Self {
        let mut this = Scope::default();

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
                AmlTerm::Mutex(name, num) => {
                    handle_add(
                        current_path,
                        &mut this,
                        root,
                        name,
                        ElementType::Mutex(*num),
                    );
                }
                _ => {
                    warn!("Should not be in root terms {term:?}");
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
                    let ElementType::ScopeOrDevice(element) = element else {
                        panic!("New element: {name:?} is not a scope or device");
                    };
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
}

fn display_scope(
    name: &str,
    scope: &Scope,
    f: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    writeln!(f, "Scope ({}) {{", name)?;
    for (name, element) in &scope.children {
        parser::display_depth(f, depth + 1)?;
        match element {
            ElementType::ScopeOrDevice(scope) => display_scope(name, scope, f, depth + 1)?,
            ElementType::Method(method) => {
                parser::display_method(method, f, depth + 1)?;
            }
            ElementType::Processor(processor) => {
                writeln!(
                    f,
                    "Processor ({}, 0x{:02X}, 0x{:04X}, 0x{:02X}) {{",
                    processor.name, processor.unk1, processor.unk2, processor.unk3
                )?;
                parser::display_terms(&processor.term_list, f, depth + 2)?;
                parser::display_depth(f, depth + 1)?;
                writeln!(f, "}}")?;
            }
            ElementType::PowerResource(power_resource) => {
                writeln!(
                    f,
                    "PowerResource ({}, 0x{:02X}, 0x{:04X}) {{",
                    power_resource.name, power_resource.system_level, power_resource.resource_order,
                )?;
                parser::display_terms(&power_resource.term_list, f, depth + 1)?;
                parser::display_depth(f, depth)?;
                writeln!(f, "}}")?;
            }
            ElementType::RegionFields(region, fields) => {
                if let Some(region) = region {
                    write!(f, "Region ({}, {}, ", region.name, region.region_space,)?;
                    parser::display_term_arg(&region.region_offset, f, depth)?;
                    write!(f, ", ")?;
                    parser::display_term_arg(&region.region_length, f, depth)?;
                    write!(f, ")")?;
                } else {
                    write!(f, "REGION {name:?} NOT FOUND!!!!")?;
                }

                writeln!(f)?;
                if fields.is_empty() {
                    write!(f, "NO FIELDS for {name:?} !!! ")?;
                }
                for field in fields {
                    parser::display_depth(f, depth + 1)?;
                    writeln!(f, "Field ({}, {}) {{", field.name, field.flags)?;
                    parser::display_fields(&field.fields, f, depth + 2)?;
                    parser::display_depth(f, depth + 1)?;
                    writeln!(f, "}}")?;
                }
            }
            ElementType::IndexField(index_field) => {
                writeln!(
                    f,
                    "IndexField ({}, {}, {}) {{",
                    index_field.name, index_field.index_name, index_field.flags
                )?;
                parser::display_fields(&index_field.fields, f, depth + 2)?;
                parser::display_depth(f, depth + 1)?;
                writeln!(f, "}}")?;
            }
            ElementType::Name(data_object) => {
                write!(f, "Name({}, ", name)?;
                parser::display_data_object(data_object, f, depth + 1)?;
                write!(f, ")")?;
            }
            ElementType::Mutex(sync_level) => {
                write!(f, "Mutex ({}, {})", name, sync_level)?;
            }
            ElementType::UnknownElements(elements) => {
                writeln!(f, "UnknownElements ({}) {{", name)?;
                parser::display_terms(elements, f, depth + 2)?;
                writeln!(f)?;
                parser::display_depth(f, depth + 1)?;
                write!(f, "}}")?;
            }
        }
        writeln!(f)?;
    }
    parser::display_depth(f, depth)?;
    writeln!(f, "}}")
}

impl StructuredAml {
    #[allow(dead_code)]
    pub fn display_with_depth(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        parser::display_depth(f, depth)?;
        display_scope("\\", &self.root, f, depth)
    }
}

testing::test! {
    fn test_structure() {
        use super::parser::{DataObject, FieldElement, IntegerData, ScopeObj, Target, TermArg};
        use alloc::boxed::Box;

        let code = AmlCode {
            term_list: vec![
                AmlTerm::Scope(ScopeObj {
                    name: "\\".to_string(),
                    term_list: vec![
                        AmlTerm::Region(RegionObj {
                            name: "DBG_".to_string(),
                            region_space: 1,
                            region_offset: TermArg::DataObject(DataObject::Integer(
                                IntegerData::WordConst(1026),
                            )),
                            region_length: TermArg::DataObject(DataObject::Integer(
                                IntegerData::ConstOne,
                            )),
                        }),
                        AmlTerm::Field(FieldDef {
                            name: "DBG_".to_string(),
                            flags: 1,
                            fields: vec![FieldElement::Named("DBGB".to_string(), 8)],
                        }),
                        AmlTerm::Method(MethodObj {
                            name: "DBUG".to_string(),
                            flags: 1,
                            term_list: vec![
                                AmlTerm::ToHexString(TermArg::Arg(0), Box::new(Target::Local(0))),
                                AmlTerm::ToBuffer(TermArg::Local(0), Box::new(Target::Local(0))),
                            ],
                        }),
                    ],
                }),
                AmlTerm::Method(MethodObj {
                    name: "\\_GPE._E02".to_string(),
                    flags: 0,
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
