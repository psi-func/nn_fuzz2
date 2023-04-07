use std::fmt::Display;

use postcard::Error as PostcardError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    SerializeError(String),
    IOError(String),
    NotAvailable(),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Error::SerializeError(msg) => format!("Error while serialize message cause: {msg}"),
                Error::IOError(msg) => format!("IO Error cause: {msg}"),
                Error::NotAvailable() => "Not Available to read".to_string(),
            }
        )
    }
}

impl Error {
    #[must_use]
    pub fn io_error(msg: String) -> Self {
        Self::IOError(msg)
    }

    #[must_use]
    pub fn serialize_error(msg: String) -> Self {
        Self::SerializeError(msg)
    }

    #[must_use]
    pub fn not_available() -> Self {
        Self::NotAvailable()
    }
}

impl From<PostcardError> for Error {
    fn from(value: PostcardError) -> Self {
        Self::serialize_error(value.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::io_error(value.to_string())
    }
}
