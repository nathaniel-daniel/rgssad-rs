use crate::sans_io::FileHeader3;
use crate::sans_io::ReaderAction3;
use crate::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

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

    /// Read the next file from this archive.
    pub fn read_file(&mut self) -> Result<Option<File<R>>, Error> {
        loop {
            match self.state_machine.step_read_file_header()? {
                ReaderAction3::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);
                }
                ReaderAction3::Seek(position) => {
                    self.reader.seek(SeekFrom::Start(position))?;
                    self.state_machine.finish_seek(position);
                }
                ReaderAction3::Done(file_header) => {
                    let file_header = match file_header {
                        Some(file_header) => file_header,
                        None => return Ok(None),
                    };

                    return Ok(Some(File {
                        reader: &mut self.reader,
                        state_machine: &mut self.state_machine,

                        header: file_header,
                    }));
                }
            }
        }
    }
}

/// A file
#[derive(Debug)]
pub struct File<'a, R> {
    reader: &'a mut R,
    state_machine: &'a mut crate::sans_io::Reader3,

    header: FileHeader3,
}

impl<R> File<'_, R> {
    /// The file path
    pub fn name(&self) -> &str {
        self.header.name.as_str()
    }

    /// The file size
    pub fn size(&self) -> u32 {
        self.header.size
    }

    /// The file key
    pub fn key(&self) -> u32 {
        self.header.key
    }
}

impl<R> Read for File<'_, R>
where
    R: Read + Seek,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let action = self
                .state_machine
                .step_read_file_data(&self.header, buffer)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

            match action {
                ReaderAction3::Read(size) => {
                    let space = self.state_machine.space();

                    // Even if we read shorter than requested,
                    // the state machine is tolerant to this
                    // and will request another read if needed.
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);
                }
                ReaderAction3::Seek(position) => {
                    self.reader.seek(SeekFrom::Start(position))?;
                    self.state_machine.finish_seek(position);
                }
                ReaderAction3::Done(n) => return Ok(n),
            }
        }
    }
}
