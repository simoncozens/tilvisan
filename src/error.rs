use skrifa::{raw::ReadError, Tag};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutohintError {
    #[error("Font read error: {0}")]
    FontReadError(#[from] ReadError),
    #[error("Font write error: {0}")]
    FontWriteError(#[from] write_fonts::error::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Missing required table: {0}")]
    MissingTable(Tag),
    #[error("Invalid font: {0}")]
    InvalidFont(&'static str),
    #[error("Unported error with code: {0}")]
    UnportedError(i32),
    #[error("Null pointer error")]
    NullPointer,
    #[error("Invalid font table")]
    InvalidTable,
    #[error("Cancelled during progress callback")]
    ProgressCancelled,
    #[error("Bad control file: {0}")]
    BadControlFile(String),
    #[error("Table already processed")]
    TableAlreadyProcessed,

    #[error("Control file parse error at line {line}, column {column}: {message}")]
    ControlFileParseError {
        message: String,
        line: usize,
        column: usize,
    },
    #[error("Control file validation error at entry {entry_index}: {message}")]
    ControlFileValidationError { entry_index: usize, message: String },
}
