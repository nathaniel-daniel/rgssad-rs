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

    /// A file name was invalid.
    InvalidFileName {
        /// The error
        error: std::string::FromUtf8Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_error) => write!(f, "an I/O error occured"),
            Self::InvalidMagic { magic } => write!(f, "magic number \"{magic:?}\" is invalid"),
            Self::InvalidVersion { version } => write!(f, "version \"{version}\" is invalid"),
            Self::InvalidFileName { .. } => write!(f, "invalid file name"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidFileName { error } => Some(error),
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

    const VX_TEST_GAME: &str = "test_data/RPGMakerVXTestGame-Export/RPGMakerVXTestGame/Game.rgss2a";

    #[test]
    fn reader_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file).expect("failed to create reader");

        // Ensure skipping works.
        while let Some(_entry) = reader.read_entry().expect("failed to read entry") {}

        //let mut entries = Vec::new();
        while let Some(entry) = reader.read_entry().expect("failed to read entry") {
            //let mut buffer = Vec::new();
            /*
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
            */
        }
    }
}
