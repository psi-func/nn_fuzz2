use std::io;

#[derive(Debug, Clone)]
pub enum Error {
    IO(String),
    NotAvailable(),
    StopIteration(),
    InvalidFormat(String),
    IllegalState(String),
    Serialize(String),
    Compression(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => {
                writeln!(f, "IO error: {e}")
            }
            Self::InvalidFormat(e) => {
                writeln!(f, "Invalid format error: {e}")
            }
            Self::IllegalState(e) => {
                writeln!(f, "Illegal program state: {e}")
            }
            Self::Serialize(e) => {
                writeln!(f, "Serialization error: {e}")
            }
            Self::Compression(e) => {
                writeln!(f, "Compression error: {e}")
            }
            Self::NotAvailable() => {
                writeln!(f, "Resource is unavailable")
            }
            Self::StopIteration() => {
                writeln!(f, "python stop iteration")
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
    pub fn stop_iteration() -> Self {
        Self::StopIteration()
    }

    #[must_use]
    pub fn illegal_state(e: String) -> Self {
        Self::IllegalState(e)
    }

    #[must_use]
    pub fn serialize(e: String) -> Self {
        Self::Serialize(e)
    }

    #[must_use]
    pub fn io(e: String) -> Self {
        Self::IO(e)
    }

    #[must_use]
    pub fn compression(e: String) -> Self {
        Self::Compression(e)
    }

    #[must_use]
    pub fn not_available() -> Self {
        Self::NotAvailable()
    }
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Self::serialize(e.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::io(e.to_string())
    }
}

impl From<libafl::Error> for Error {
    fn from(e: libafl::Error) -> Self {
        match e {
            libafl::Error::Compression(_) => {
                Self::compression("error while compressing buffer".to_string())
            }
            _ => {
                unreachable!()
            }
        }
    }
}

impl From<nn_messages::error::Error> for Error {
    fn from(e: nn_messages::error::Error) -> Self {
        match e {
            nn_messages::error::Error::SerializeError(msg) => Self::serialize(msg),
            nn_messages::error::Error::IOError(msg) => Self::io(msg),
            nn_messages::error::Error::NotAvailable() => Self::not_available(),
        }
    }
}
