// #[warn(clippy::arithmetic_side_effects)]

/// The archive reader.
pub mod reader;
/// sans-io state machines for reading and writing.
pub mod sans_io;
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

/// The len of the magic number.
const MAGIC_LEN: usize = 7;
/// The magic number
const MAGIC: [u8; MAGIC_LEN] = *b"RGSSAD\0";
/// The file version
const VERSION: u8 = 1;
/// The size of the header.
const HEADER_LEN: usize = MAGIC_LEN + 1;
/// The default encryption key
const DEFAULT_KEY: u32 = 0xDEADCAFE;
/// The maximum file name len
const MAX_FILE_NAME_LEN: u32 = 4096;
/// The size of a u32, in bytes.
const U32_LEN: usize = 4;

/// The library error type
#[derive(Debug)]
pub enum Error {
    /// An I/O error occured.
    Io(std::io::Error),

    /// Invalid internal state, user error
    InvalidState,

    /// The file name was too long
    FileNameTooLong {
        /// The error
        error: std::num::TryFromIntError,
    },

    /// The provided file size does not match the file data's size.
    FileDataSizeMismatch { actual: u32, expected: u32 },

    /// The file data was too long
    FileDataTooLong,

    /// There was an error with the sans-io state machine.
    SansIo(self::sans_io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_error) => write!(f, "an I/O error occured"),
            Self::InvalidState => write!(f, "user error, invalid internal state"),

            Self::FileNameTooLong { .. } => write!(f, "the file name is too long"),

            Self::FileDataSizeMismatch { actual, expected } => write!(
                f,
                "file data size mismatch, expected {expected} but got {actual}"
            ),
            Self::FileDataTooLong => write!(f, "file data too long"),
            Self::SansIo(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::FileNameTooLong { error } => Some(error),

            Self::SansIo(error) => error.source(),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<self::sans_io::Error> for Error {
    fn from(error: self::sans_io::Error) -> Self {
        Self::SansIo(error)
    }
}

/// Encrypt or decrypt an u32, and rotate the key as needed.
fn crypt_u32(key: &mut u32, mut n: u32) -> u32 {
    n ^= *key;
    *key = key.overflowing_mul(7).0.overflowing_add(3).0;
    n
}

fn crypt_name_bytes(key: &mut u32, bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // We mask with 0xFF, this cannot exceed the bounds of a u8.
        *byte ^= u8::try_from(*key & 0xFF).unwrap();
        *key = key.overflowing_mul(7).0.overflowing_add(3).0;
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

        // Read entire archive into a Vec.
        let mut files = Vec::new();
        while let Some(mut file) = reader.read_file().expect("failed to read file") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).expect("failed to read file");
            files.push((file.name().to_string(), buffer));
        }

        // Write all files into a new archive.
        let mut new_file = Vec::new();
        let mut writer = Writer::new(&mut new_file);
        writer.write_header().expect("failed to write header");
        for (file_name, file_data) in files.iter() {
            writer
                .write_entry(
                    file_name,
                    u32::try_from(file_data.len()).expect("file data too large"),
                    &**file_data,
                )
                .expect("failed to write file");
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
            match reader.read_file() {
                Ok(Some(_file)) => {}
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
        let mut files = Vec::new();
        while let Some(mut file) = reader.read_file().expect("failed to read file") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).expect("failed to read file");
            files.push((file.name().to_string(), buffer));
        }

        // Write all files into new archive.
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

        for (file_name, file_data) in files.iter() {
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
                        panic!("failed to write file: {error}");
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
