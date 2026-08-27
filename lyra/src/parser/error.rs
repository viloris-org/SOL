use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),

    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Expected {expected}, found {found}")]
    ExpectedToken { expected: String, found: String },

    #[error("Invalid syntax: {0}")]
    InvalidSyntax(String),
}

pub type ParseResult<T> = Result<T, ParseError>;
