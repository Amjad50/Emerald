mod parser;
mod structured;

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
}
