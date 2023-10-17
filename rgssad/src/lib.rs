mod reader;

pub use self::reader::Reader;

/// The magic number
const MAGIC: &[u8] = b"RGSSAD\0";
/// The file version
const VERSION: u8 = 1;
/// The default encryption key
const DEFAULT_KEY: u32 = 0xDEADCAFE;

/// The library error type
#[derive(Debug)]
pub enum Error {
    /// An I/O error occured.
    Io(std::io::Error),

    /// Invalid magic number
    InvalidMagic { magic: [u8; 7] },

    /// Invalid version
    InvalidVersion { version: u8 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_error) => write!(f, "an I/O error occured"),
            Self::InvalidMagic { magic } => write!(f, "magic number \"{magic:?}\" is invalid"),
            Self::InvalidVersion { version } => write!(f, "version \"{version}\" is invalid"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn reader_smoke() {
        // PASS
    }
}
