use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    acpi::aml::{parser::PackageElement, structured::ElementType},
    testing,
};

use super::{
    parser::{IntegerData, TermArg, UnresolvedDataObject},
    structured::{StructuredAml, StructuredAmlError},
};

#[derive(Debug, Clone)]
pub struct Package {
    size: IntegerData,
    elements: Vec<PackageElement<DataObject>>,
}

#[allow(dead_code)]
impl Package {
    pub fn size(&self) -> usize {
        self.size.as_u64() as usize
    }

    pub fn get(&self, index: usize) -> Option<&PackageElement<DataObject>> {
        self.elements.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PackageElement<DataObject>> {
        self.elements.iter()
    }
}

/// A version of [UnresolvedDataObject] that is after execution
/// so it doesn't have dynamic contents, references, or expressions
#[derive(Debug, Clone)]
pub enum DataObject {
    Integer(IntegerData),
    Buffer(IntegerData, Vec<u8>),
    Package(Package),
    String(String),
    EisaId(String),
}

#[allow(dead_code)]
impl DataObject {
    pub fn as_integer(&self) -> Option<&IntegerData> {
        match self {
            Self::Integer(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_package(&self) -> Option<&Package> {
        match self {
            Self::Package(package) => Some(package),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AmlExecutionError {
    LableNotFound(String),
    StructuredAmlError(StructuredAmlError),
    ElementNotExecutable(String),
    UnexpectedTermResultType(TermArg, String),
}

impl From<StructuredAmlError> for AmlExecutionError {
    fn from(err: StructuredAmlError) -> Self {
        Self::StructuredAmlError(err)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExecutionContext {}
impl ExecutionContext {
    pub fn execute(
        &self,
        structured: &StructuredAml,
        label: &str,
        _args: &[UnresolvedDataObject],
    ) -> Result<DataObject, AmlExecutionError> {
        let element_to_execute = structured
            .find_object(label)?
            .ok_or(AmlExecutionError::LableNotFound(label.to_string()))?;

        let data = match element_to_execute {
            ElementType::Method(_) => todo!("Execute method"),
            ElementType::Name(data) => data,
            ElementType::UnknownElements(_) => {
                // This label is internal and should never be reached
                return Err(AmlExecutionError::LableNotFound(label.to_string()));
            }
            ElementType::PowerResource(_)
            | ElementType::RegionFields(_, _)
            | ElementType::IndexField(_)
            | ElementType::ScopeOrDevice(_)
            | ElementType::Processor(_) => {
                return Err(AmlExecutionError::ElementNotExecutable(label.to_string()))
            }
        };

        self.evaluate_data_object(data.clone(), label)
    }

    fn execute_term_arg(
        &self,
        term: &TermArg,
        _reference_path: &str,
    ) -> Result<DataObject, AmlExecutionError> {
        todo!("Execute term: {:?}", term)
    }

    fn convert_package_elements(
        &self,
        elements: Vec<PackageElement<UnresolvedDataObject>>,
        reference_path: &str,
    ) -> Result<Vec<PackageElement<DataObject>>, AmlExecutionError> {
        elements
            .into_iter()
            .map(|e| {
                Ok(match e {
                    PackageElement::DataObject(data) => {
                        PackageElement::DataObject(self.evaluate_data_object(data, reference_path)?)
                    }
                    PackageElement::Name(name) => PackageElement::Name(name),
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn evaluate_data_object(
        &self,
        data: UnresolvedDataObject,
        reference_path: &str,
    ) -> Result<DataObject, AmlExecutionError> {
        match data {
            UnresolvedDataObject::Buffer(buffer) => {
                let size_term = self.execute_term_arg(buffer.size.as_ref(), reference_path)?;

                let size_term = match size_term {
                    DataObject::Integer(i) => i,
                    _ => {
                        return Err(AmlExecutionError::UnexpectedTermResultType(
                            buffer.size.as_ref().clone(),
                            "Integer".to_string(),
                        ))
                    }
                };

                Ok(DataObject::Buffer(
                    size_term,
                    buffer.data.into_iter().collect(),
                ))
            }
            UnresolvedDataObject::Package(size, elements) => Ok(DataObject::Package(Package {
                size: IntegerData::ByteConst(size),
                elements: self.convert_package_elements(elements, reference_path)?,
            })),
            UnresolvedDataObject::VarPackage(term, elements) => {
                let size_term = self.execute_term_arg(term.as_ref(), reference_path)?;

                let size_term = match size_term {
                    DataObject::Integer(i) => i,
                    _ => {
                        return Err(AmlExecutionError::UnexpectedTermResultType(
                            term.as_ref().clone(),
                            "Integer".to_string(),
                        ))
                    }
                };

                Ok(DataObject::Package(Package {
                    size: size_term,
                    elements: self.convert_package_elements(elements, reference_path)?,
                }))
            }
            UnresolvedDataObject::Integer(i) => Ok(DataObject::Integer(i)),
            UnresolvedDataObject::String(s) => Ok(DataObject::String(s)),
            UnresolvedDataObject::EisaId(s) => Ok(DataObject::EisaId(s)),
        }
    }
}

testing::test! {
    /// Test executing and getting data from
    /// ```
    /// Name("_S5_", Package(4) {0x5, 0x5, Zero, Zero}
    /// Name("_S4_", Package(4) {0x4, 0x4, Zero, Zero}
    /// ```
    /// ```
    fn test_execute_normal_sleep_package() {
        use super::parser::{AmlCode, AmlTerm};
        use alloc::vec;

        fn return_package_of_name(
            ctx: &mut ExecutionContext,
            structured_code: &StructuredAml,
            name: &str,
        ) -> Vec<u8> {
            ctx.execute(structured_code, name, &[])
                .expect("label")
                .as_package()
                .expect("package")
                .iter()
                .map(|d| {
                    d.as_data()
                        .expect("data")
                        .as_integer()
                        .expect("integer")
                        .as_u8()
                        .unwrap()
                })
                .collect::<Vec<_>>()
        }

        let code = AmlCode {
            term_list: vec![
                AmlTerm::NameObj(
                    "_S5_".to_string(),
                    UnresolvedDataObject::Package(
                        4,
                        vec![
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ByteConst(5),
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ByteConst(5),
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ConstZero,
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ConstZero,
                            )),
                        ],
                    ),
                ),
                AmlTerm::NameObj(
                    "_S4_".to_string(),
                    UnresolvedDataObject::Package(
                        4,
                        vec![
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ByteConst(4),
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ByteConst(4),
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ConstZero,
                            )),
                            PackageElement::DataObject(UnresolvedDataObject::Integer(
                                IntegerData::ConstZero,
                            )),
                        ],
                    ),
                ),
            ],
        };

        let structured_code = StructuredAml::parse(&code);

        let mut execution_ctx = ExecutionContext::default();

        assert_eq!(
            return_package_of_name(&mut execution_ctx, &structured_code, "\\_S5_"),
            vec![5, 5, 0, 0]
        );
        assert_eq!(
            return_package_of_name(&mut execution_ctx, &structured_code, "\\_S4_"),
            vec![4, 4, 0, 0]
        );
    }
}
