use crate::sans_io::ReaderAction3;
use crate::Error;
use std::io::Read;
use std::io::Seek;

/// A reader for a "rgss3a" archive file
#[derive(Debug)]
pub struct Reader3<R> {
    reader: R,
    state_machine: crate::sans_io::Reader3,
}

impl<R> Reader3<R> {
    /// Create a new [`Reader3`] with the default encryption key.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            state_machine: crate::sans_io::Reader3::new(),
        }
    }

    /// Get the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Get a reference to the reader.
    pub fn get_ref(&mut self) -> &R {
        &self.reader
    }

    /// Get a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R> Reader3<R>
where
    R: Read + Seek,
{
    /// Read and validate the header.
    ///
    /// After this returns, call [`Reader3::read_file`] to read through files.
    /// This function is a NOP if the header has already been read.
    pub fn read_header(&mut self) -> Result<(), Error> {
        loop {
            match self.state_machine.step_read_header()? {
                ReaderAction3::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);
                }
                ReaderAction3::Done(()) => return Ok(()),
                ReaderAction3::Seek(_) => unreachable!(),
            }
        }
    }
}
