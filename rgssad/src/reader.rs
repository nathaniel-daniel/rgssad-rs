use crate::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::VERSION;
use std::io::Read;
use std::io::Seek;

/// A reader for a "rgssad" archive file
#[allow(dead_code)]
#[derive(Debug)]
pub struct Reader<R> {
    /// The underlying reader.
    reader: R,

    key: u32,
    next_entry_position: u64,
}

impl<R> Reader<R> {
    /// Get the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> Reader<R>
where
    R: Read + Seek,
{
    /// Create a new [`Reader`] with the default encryption key.
    pub fn new(reader: R) -> Result<Self, Error> {
        let mut reader = Self {
            reader,
            key: DEFAULT_KEY,
            next_entry_position: 0,
        };
        reader.read_header()?;

        Ok(reader)
    }

    /// Read and validate the header.
    fn read_header(&mut self) -> Result<(), Error> {
        let mut magic = [0; 7];
        self.reader.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic { magic });
        }

        let mut version = 0;
        self.reader.read_exact(std::slice::from_mut(&mut version))?;
        if version != VERSION {
            return Err(Error::InvalidVersion { version });
        }

        Ok(())
    }
}
