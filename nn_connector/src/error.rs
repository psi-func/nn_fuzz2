use std::io;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub enum Error {
    IOError(String),
    NotAvailable(),
    InvalidFormat(String),
    IllegalState(String),
    SerializeError(String),
    CompressionError(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IOError(e) => {
                writeln!(f, "IO error: {e}")
            }
            Self::InvalidFormat(e) => {
                writeln!(f, "Invalid format error: {e}")
            }
            Self::IllegalState(e) => {
                writeln!(f, "Illegal program state: {e}")
            }
            Self::SerializeError(e) => {
                writeln!(f, "Serialization error: {e}")
            }
            Self::CompressionError(e) => {
                writeln!(f, "Compression error: {e}")
            },
            Self::NotAvailable() => {
                writeln!(f, "Resource is unavailable")
            }
        }
    }
}

impl Error {
    
    #[must_use]
    pub fn invalid_format(e: String) -> Self {
        Self::InvalidFormat(e)
    }
    
    #[must_use]
    pub fn illegal_state(e: String) -> Self {
        Self::IllegalState(e)
    }
    
    #[must_use]
    pub fn serialize_error(e: String) -> Self {
        Self::SerializeError(e)
    }

    #[must_use]
    pub fn io_error(e: String) -> Self {
        Self::IOError(e)
    }


    #[must_use]
    pub fn compression_error(e: String) -> Self {
        Self::CompressionError(e)
    }

    #[must_use]
    pub fn not_available() -> Self {
        Self::NotAvailable()
    }
    
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Self::serialize_error(e.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::io_error(e.to_string())
    }
}

impl From<libafl::Error> for Error {
    fn from(e: libafl::Error) -> Self {
        match e {
            libafl::Error::Compression(_) => Self::compression_error("error while compressing buffer".to_string()),
            _ => {
                unreachable!()
            }
        }
    }
}