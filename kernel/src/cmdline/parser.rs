use core::num::ParseIntError;

use super::tokenizer::Tokenizer;

#[derive(Debug)]
pub enum ParseErrorKind<'a> {
    Unexpected { need: &'a str, got: Option<&'a str> },
    ParseIntError(ParseIntError),
    UnexpectedId(&'a str),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ParseError<'a> {
    kind: ParseErrorKind<'a>,
    loc: usize,
}

impl<'a> ParseError<'a> {
    pub fn new(kind: ParseErrorKind<'a>, loc: usize) -> Self {
        Self { kind, loc }
    }
}

pub type Result<'a, T> = core::result::Result<T, ParseError<'a>>;

pub trait CmdlineParse<'a>
where
    Self: Sized,
{
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self>;
}

impl<'a> CmdlineParse<'a> for bool {
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self> {
        let (loc, value) = tokenizer.next_value().ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "true/false",
                    got: None,
                },
                tokenizer.current_index(),
            )
        })?;

        match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "true/false",
                    got: Some(value),
                },
                loc,
            )),
        }
    }
}

impl<'a> CmdlineParse<'a> for u32 {
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self> {
        let (loc, value) = tokenizer.next_value().ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "<number>",
                    got: None,
                },
                tokenizer.current_index(),
            )
        })?;

        value
            .parse()
            .map_err(|e| ParseError::new(ParseErrorKind::ParseIntError(e), loc))
    }
}

impl<'a> CmdlineParse<'a> for &'a str {
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self> {
        let (_loc, value) = tokenizer.next_value().ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "<str>",
                    got: None,
                },
                tokenizer.current_index(),
            )
        })?;

        Ok(value)
    }
}
