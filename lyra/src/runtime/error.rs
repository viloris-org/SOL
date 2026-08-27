use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),
    
    #[error("Undefined command: {0}")]
    UndefinedCommand(String),
    
    #[error("Type error: expected {expected}, got {got}")]
    TypeError { expected: String, got: String },
    
    #[error("Arity error: expected {expected} arguments, got {got}")]
    ArityError { expected: usize, got: usize },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("{0}")]
    Custom(String),
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;
