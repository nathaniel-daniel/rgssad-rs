mod reader;

pub use self::reader::Reader;

/// An error that may occur while using sans-io state machines.
#[derive(Debug)]
pub enum Error {
    /// The file was expected to continue, but it did not.
    UnexpectedEof,

    /// Invalid magic number
    InvalidMagic { magic: [u8; 7] },

    /// Invalid version
    InvalidVersion { version: u8 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected eof"),
            Self::InvalidMagic { magic } => write!(f, "magic number \"{magic:?}\" is invalid"),
            Self::InvalidVersion { version } => write!(f, "version \"{version}\" is invalid"),
        }
    }
}

impl std::error::Error for Error {}

/// An action that should be performed for the state machine, or a result.
#[derive(Debug)]
pub enum SansIoAction<T> {
    /// Read at least the given number of bytes before stepping again.
    Read(usize),

    /// Seek to the given position before stepping again.
    Seek(usize),

    /// The stepping function is done.
    Done(T),
}

impl<T> SansIoAction<T> {
    /// Returns true if this is a `Done` variant.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }
}
