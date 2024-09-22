use crate::sans_io::ReaderAction;
use crate::Error;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

/// A reader for a "rgssad" archive file
#[derive(Debug)]
pub struct Reader<R> {
    reader: R,
    state_machine: crate::sans_io::Reader,
}

impl<R> Reader<R> {
    /// Create a new [`Reader`] with the default encryption key.
    pub fn new(reader: R) -> Reader<R> {
        Reader {
            reader,
            state_machine: crate::sans_io::Reader::new(),
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

impl<R> Reader<R>
where
    R: Read + Seek,
{
    /// Read and validate the header.
    ///
    /// After this returns, call [`Reader::read_file`] to read through files.
    /// This function is a NOP if the header has already been read.
    pub fn read_header(&mut self) -> Result<(), Error> {
        loop {
            match self.state_machine.step_read_header()? {
                ReaderAction::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);
                }
                ReaderAction::Done(()) => return Ok(()),
                ReaderAction::Seek(_) => unreachable!(),
            }
        }
    }

    /// Read the next file from this archive.
    pub fn read_file(&mut self) -> Result<Option<File<R>>, Error> {
        loop {
            match self.state_machine.step_read_file_header()? {
                ReaderAction::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);

                    if n == 0 {
                        if self.state_machine.available_data() == 0 {
                            return Ok(None);
                        } else {
                            return Err(Error::Io(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "failed to fill whole buffer",
                            )));
                        }
                    }
                }
                ReaderAction::Seek(position) => {
                    self.reader.seek(SeekFrom::Start(position))?;
                    self.state_machine.finish_seek();
                }
                ReaderAction::Done(file_header) => {
                    let size = file_header.size;
                    return Ok(Some(File {
                        name: file_header.name,
                        size,
                        state_machine: &mut self.state_machine,
                        reader: &mut self.reader,
                    }));
                }
            }
        }
    }
}

/// An file in an rgssad file
#[derive(Debug)]
pub struct File<'a, R> {
    /// The file path.
    name: String,

    /// The file size.
    size: u32,

    reader: &'a mut R,
    state_machine: &'a mut crate::sans_io::Reader,
}

impl<R> File<'_, R> {
    /// The file path
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// The file size
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<R> Read for File<'_, R>
where
    R: Read,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let action = self
                .state_machine
                .step_read_file_data(buffer)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

            match action {
                ReaderAction::Read(size) => {
                    let space = self.state_machine.space();

                    // Even if we read shorter than requested,
                    // the state machine is tolerant to this
                    // and will request another read if needed.
                    let n = self.reader.read(&mut space[..size])?;
                    self.state_machine.fill(n);
                }
                ReaderAction::Seek(_) => unreachable!(),
                ReaderAction::Done(n) => return Ok(n),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::*;

    #[test]
    fn reader_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Ensure skipping works.
        let mut num_skipped_entries = 0;
        while let Some(_file) = reader.read_file().expect("failed to read file") {
            num_skipped_entries += 1;
        }

        // Reset position and recreate reader.
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Read entire archive into a Vec.
        let mut files = Vec::new();
        while let Some(mut file) = reader.read_file().expect("failed to read file") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).expect("failed to read file");
            files.push((file.name().to_string(), buffer));
        }

        assert!(files.len() == num_skipped_entries);
    }

    #[test]
    fn reader_trailing_bytes() {
        let mut file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        file.push(1);
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        while let Ok(Some(_file)) = reader.read_file() {}

        let error = reader.read_file().expect_err("reader should have errored");
        assert!(
            matches!(error, Error::Io(error) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}
