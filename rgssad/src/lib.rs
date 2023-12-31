// #[warn(clippy::arithmetic_side_effects)]

/// The archive reader.
pub mod reader;
/// The archive writer.
pub mod writer;

/// Tokio adapters for archive readers and writers.
#[cfg(feature = "tokio")]
pub mod tokio;

pub use self::reader::Reader;
#[cfg(feature = "tokio")]
pub use self::tokio::TokioReader;
#[cfg(feature = "tokio")]
pub use self::tokio::TokioWriter;
pub use self::writer::Writer;

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

    /// Invalid internal state, user error
    InvalidState,

    /// Invalid magic number
    InvalidMagic { magic: [u8; 7] },

    /// Invalid version
    InvalidVersion { version: u8 },

    /// The file name was too long
    FileNameTooLong {
        /// The error
        error: std::num::TryFromIntError,
    },

    /// A file name was invalid.
    InvalidFileName {
        /// The error
        error: std::string::FromUtf8Error,
    },

    /// The provided file size does not match the file data's size.
    FileDataSizeMismatch { actual: u32, expected: u32 },

    /// The file data was too long
    FileDataTooLong,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_error) => write!(f, "an I/O error occured"),
            Self::InvalidState => write!(f, "user error, invalid internal state"),
            Self::InvalidMagic { magic } => write!(f, "magic number \"{magic:?}\" is invalid"),
            Self::InvalidVersion { version } => write!(f, "version \"{version}\" is invalid"),
            Self::FileNameTooLong { .. } => write!(f, "the file name is too long"),
            Self::InvalidFileName { .. } => write!(f, "invalid file name"),
            Self::FileDataSizeMismatch { actual, expected } => write!(
                f,
                "file data size mismatch, expected {expected} but got {actual}"
            ),
            Self::FileDataTooLong => write!(f, "file data too long"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::FileNameTooLong { error } => Some(error),
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
    use std::cell::RefCell;
    use std::io::Read;
    use std::io::Seek;
    use std::io::SeekFrom;
    use std::io::Write;
    use std::rc::Rc;

    pub const VX_TEST_GAME: &str =
        "test_data/RPGMakerVXTestGame-Export/RPGMakerVXTestGame/Game.rgss2a";

    #[derive(Debug, Clone)]
    struct SlowReader<R> {
        inner: Rc<RefCell<(R, usize, Option<SeekFrom>)>>,
    }

    impl<R> SlowReader<R> {
        pub fn new(reader: R) -> Self {
            Self {
                inner: Rc::new(RefCell::new((reader, 0, None))),
            }
        }

        fn add_fuel(&self, fuel: usize) {
            let mut inner = self.inner.borrow_mut();
            inner.1 += fuel;
        }
    }

    impl<R> Read for SlowReader<R>
    where
        R: Read,
    {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut inner = self.inner.borrow_mut();
            let (reader, fuel, _) = &mut *inner;

            assert!(!buf.is_empty());
            let limit = std::cmp::min(*fuel, buf.len());
            let buf = &mut buf[..limit];
            if buf.is_empty() {
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            }
            let n = reader.read(buf)?;
            *fuel -= n;

            Ok(n)
        }
    }

    impl<R> Seek for SlowReader<R>
    where
        R: Seek,
    {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            let mut inner = self.inner.borrow_mut();
            let (reader, _, seek_request) = &mut *inner;
            match seek_request {
                Some(seek_request) => {
                    assert!(pos == *seek_request, "{pos:?} != {seek_request:?}");
                }
                None => {
                    *seek_request = Some(pos);
                    return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
                }
            }

            let result = reader.seek(pos);
            *seek_request = None;

            result
        }
    }

    #[derive(Debug, Clone)]
    struct SlowWriter<W> {
        inner: Rc<RefCell<(W, usize, bool)>>,
    }

    impl<W> SlowWriter<W> {
        pub fn new(writer: W) -> Self {
            Self {
                inner: Rc::new(RefCell::new((writer, 0, false))),
            }
        }

        fn add_fuel(&self, fuel: usize) {
            let mut inner = self.inner.borrow_mut();
            inner.1 += fuel;
        }
    }

    impl<W> Write for SlowWriter<W>
    where
        W: Write,
    {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut inner = self.inner.borrow_mut();
            let (writer, fuel, _) = &mut *inner;

            if *fuel == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            }

            let len = std::cmp::min(*fuel, buf.len());
            let n = writer.write(&buf[..len])?;
            *fuel -= n;

            Ok(n)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            let mut inner = self.inner.borrow_mut();
            let (writer, _, should_flush) = &mut *inner;
            if *should_flush {
                writer.flush()?;
                *should_flush = false;

                Ok(())
            } else {
                *should_flush = true;

                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
            }
        }
    }

    #[test]
    fn reader_writer_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Read entire archive into Vec.
        let mut entries = Vec::new();
        while let Some(mut entry) = reader.read_entry().expect("failed to read entry") {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
        }

        // Write all entries into new archive.
        let mut new_file = Vec::new();
        let mut writer = Writer::new(&mut new_file);
        writer.write_header().expect("failed to write header");
        for (file_name, file_data) in entries.iter() {
            writer
                .write_entry(
                    file_name,
                    u32::try_from(file_data.len()).expect("file data too large"),
                    &**file_data,
                )
                .expect("failed to write entry");
        }
        writer.finish().expect("failed to flush");

        let file = reader.into_inner();

        // Ensure archives are byte-for-byte equivalent.
        assert!(&new_file == file.get_ref());
    }

    #[test]
    fn slow_reader() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let file = SlowReader::new(file);
        let mut reader = Reader::new(file.clone());

        loop {
            match reader.read_header() {
                Ok(()) => break,
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    panic!("Error: {error:?}");
                }
            }

            file.add_fuel(1);
        }

        loop {
            match reader.read_entry() {
                Ok(Some(_entry)) => {}
                Ok(None) => break,
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    panic!("Error: {error:?}");
                }
            }

            file.add_fuel(1);
        }
    }

    #[test]
    fn reader_slow_writer_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Read entire archive into Vec.
        let mut entries = Vec::new();
        while let Some(mut entry) = reader.read_entry().expect("failed to read entry") {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
        }

        // Write all entries into new archive.
        let new_file = SlowWriter::new(Vec::<u8>::new());
        let mut writer = Writer::new(new_file.clone());
        loop {
            match writer.write_header() {
                Ok(()) => break,
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    new_file.add_fuel(1);
                }
                Err(error) => {
                    panic!("failed to write header: {error}");
                }
            }
        }

        for (file_name, file_data) in entries.iter() {
            let len = u32::try_from(file_data.len()).expect("file data too large");
            // We need to pass the same reader, so that updates to its position are persisted.
            let mut reader = &**file_data;

            loop {
                match writer.write_entry(file_name, len, &mut reader) {
                    Ok(()) => break,
                    Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        new_file.add_fuel(1);
                    }
                    Err(error) => {
                        panic!("failed to write entry: {error}");
                    }
                }
            }
        }
        loop {
            match writer.finish() {
                Ok(()) => break,
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    panic!("failed to flush: {error}");
                }
            }
        }

        let file = reader.into_inner();

        // Ensure archives are byte-for-byte equivalent.
        assert!(&new_file.inner.borrow().0 == file.get_ref());
    }
}
