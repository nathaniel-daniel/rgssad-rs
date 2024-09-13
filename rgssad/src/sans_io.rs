mod reader;
mod writer;

pub use self::reader::Reader;
pub use self::writer::Writer;
use crate::MAX_FILE_NAME_LEN;

/// An error that may occur while using sans-io state machines.
#[derive(Debug)]
pub enum Error {
    /// Invalid magic number
    InvalidMagic { magic: [u8; 7] },

    /// Invalid version
    InvalidVersion { version: u8 },

    /// File name was too long
    FileNameTooLongU32 { len: u32 },

    /// File name was too long
    FileNameTooLongUsize { len: usize },

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
            Self::FileNameTooLongU32 { len } => write!(
                f,
                "file name {len} is too long, max length is {MAX_FILE_NAME_LEN}"
            ),
            Self::FileNameTooLongUsize { len } => write!(
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

/// An action that should be performed for the reader state machine, or a result.
#[derive(Debug, Copy, Clone)]
pub enum ReaderAction<T> {
    /// Read at least the given number of bytes before stepping again.
    Read(usize),

    /// Seek to the given position before stepping again.
    Seek(u64),

    /// The stepping function is done.
    Done(T),
}

impl<T> ReaderAction<T> {
    /// Returns true if this is a `Done` variant.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    /// Map the done variant.
    fn map_done<F, O>(self, f: F) -> ReaderAction<O>
    where
        F: FnOnce(T) -> O,
    {
        match self {
            Self::Read(n) => ReaderAction::Read(n),
            Self::Seek(p) => ReaderAction::Seek(p),
            Self::Done(v) => ReaderAction::Done(f(v)),
        }
    }
}

/// An action that should be performed for the writer state machine, or a result..
#[derive(Debug, Copy, Clone)]
pub enum WriterAction<T> {
    /// The writer buffer should be emptied.
    Write,

    /// The stepping function is done.
    Done(T),
}

impl<T> WriterAction<T> {
    /// Returns true if this is a `Done` variant.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }
}

/// A file header
#[derive(Debug)]
pub struct FileHeader {
    /// The file name
    pub name: String,

    /// The file data size.
    pub size: u32,
}
