mod reader;

pub use self::reader::FileHeader;
pub use self::reader::Reader;
use crate::MAX_FILE_NAME_LEN;

/// An error that may occur while using sans-io state machines.
#[derive(Debug)]
pub enum Error {
    /// Invalid magic number
    InvalidMagic { magic: [u8; 7] },

    /// Invalid version
    InvalidVersion { version: u8 },

    /// File name was too long
    FileNameTooLong { len: u32 },

    /// A file name was invalid.
    InvalidFileName {
        /// The error
        error: std::string::FromUtf8Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InvalidMagic { magic } => write!(f, "magic number \"{magic:?}\" is invalid"),
            Self::InvalidVersion { version } => write!(f, "version \"{version}\" is invalid"),
            Self::FileNameTooLong { len } => write!(
                f,
                "file name {len} is too long, max length is {MAX_FILE_NAME_LEN}"
            ),
            Self::InvalidFileName { .. } => write!(f, "invalid file name"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidFileName { error } => Some(error),

            _ => None,
        }
    }
}

/// An action that should be performed for the state machine, or a result.
#[derive(Debug)]
pub enum Action<T> {
    /// Read at least the given number of bytes before stepping again.
    Read(usize),

    /// Seek to the given position before stepping again.
    Seek(u64),

    /// The stepping function is done.
    Done(T),
}

impl<T> Action<T> {
    /// Returns true if this is a `Done` variant.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    /// Map the done variant.
    pub fn map_done<F, O>(self, f: F) -> Action<O>
    where
        F: FnOnce(T) -> O,
    {
        match self {
            Self::Read(n) => Action::Read(n),
            Self::Seek(p) => Action::Seek(p),
            Self::Done(v) => Action::Done(f(v)),
        }
    }
}
