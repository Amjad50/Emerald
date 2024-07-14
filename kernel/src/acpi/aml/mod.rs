pub mod execution;
mod parser;
mod structured;

use execution::{AmlExecutionError, DataObject, ExecutionContext};
use parser::UnresolvedDataObject;

pub use parser::{AmlCode, AmlParseError};
use structured::StructuredAml;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Aml {
    code: AmlCode,
    structured: StructuredAml,
}

impl Aml {
    pub fn parse(body: &[u8]) -> Result<Self, AmlParseError> {
        let code = parser::parse_aml(body)?;
        Ok(Self {
            structured: StructuredAml::parse(&code),
            code,
        })
    }

    #[allow(dead_code)]
    pub fn code(&self) -> &AmlCode {
        &self.code
    }

    #[allow(dead_code)]
    pub fn structured(&self) -> &StructuredAml {
        &self.structured
    }

    #[allow(dead_code)]
    pub fn execute(
        &self,
        ctx: &mut ExecutionContext,
        label: &str,
        args: &[UnresolvedDataObject],
    ) -> Result<DataObject, AmlExecutionError> {
        ctx.execute(&self.structured, label, args)
    }
}
