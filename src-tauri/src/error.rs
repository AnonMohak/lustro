use serde::{Serialize, Serializer};

/// Errors that a command can return to the UI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The user cancelled the admin prompt.")]
    ElevationCancelled,
    #[error("Another scan is running. Wait for it to finish, or cancel it.")]
    Busy,
    #[error("There is no scan for this drive or folder. Scan it first.")]
    NoScan,
    #[error("{0}")]
    Windows(#[from] windows::core::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
