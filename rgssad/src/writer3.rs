use crate::sans_io::WriterAction3;
use crate::Error;
use std::io::Read;
use std::io::Write;

/// The archive writer.
#[derive(Debug)]
pub struct Writer3<W> {
    /// The inner writer.
    writer: W,

    /// The state machine
    state_machine: crate::sans_io::Writer3,

    /// The current state
    state: State,
}

impl<W> Writer3<W> {
    /// Create an archive writer around a writer.
    pub fn new(writer: W, key: u32) -> Self {
        Self {
            writer,
            state_machine: crate::sans_io::Writer3::new(key),
            state: State::FileHeader,
        }
    }

    /// Get the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Get a mutable ref to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<W> Writer3<W>
where
    W: Write,
{
    /// Write the archive header.
    ///
    /// If the header has already been written, this is a NOP.
    pub fn write_header(&mut self) -> Result<(), Error> {
        loop {
            let action = self.state_machine.step_write_header()?;
            match action {
                WriterAction3::Write => {
                    let data = self.state_machine.data();
                    let size = self.writer.write(data)?;
                    self.state_machine.consume(size);
                }
                WriterAction3::Done(()) => {
                    loop {
                        let data = self.state_machine.data();
                        if data.is_empty() {
                            break;
                        }

                        let n = self.writer.write(data)?;
                        self.state_machine.consume(n);
                    }

                    return Ok(());
                }
            }
        }
    }

    /// Add a file.
    ///
    /// This only tells the writer about the file.
    /// Writing the file metadata and data is done separately.
    /// This can only be called before file header writing begins.
    pub fn add_file(&mut self, name: String, size: u32, key: u32) -> Result<(), Error> {
        self.state_machine.add_file(name, size, key)?;
        Ok(())
    }

    /// Write the file headers.
    ///
    /// If the file headers have already been written, `Ok(WriterAction3::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically write the header is if has not been written.
    pub fn write_file_headers(&mut self) -> Result<(), Error> {
        loop {
            let action = self.state_machine.step_write_file_headers()?;
            match action {
                WriterAction3::Write => {
                    let data = self.state_machine.data();
                    let size = self.writer.write(data)?;
                    self.state_machine.consume(size);
                }
                WriterAction3::Done(()) => {
                    loop {
                        let data = self.state_machine.data();
                        if data.is_empty() {
                            break;
                        }

                        let n = self.writer.write(data)?;
                        self.state_machine.consume(n);
                    }

                    self.state = State::FileData {
                        size: 0,
                        is_new: true,
                    };

                    return Ok(());
                }
            }
        }
    }

    /// Write the file data.
    pub fn write_file_data<R>(&mut self, file_index: usize, mut file_data: R) -> Result<(), Error>
    where
        R: Read,
    {
        loop {
            match &mut self.state {
                State::FileHeader => self.write_file_headers()?,
                State::FileData { size, is_new } => {
                    *is_new = false;

                    if *size == 0 {
                        let space = loop {
                            let space = self.state_machine.space();
                            if space.is_empty() {
                                let data = self.state_machine.data();
                                let n = self.writer.write(data)?;
                                self.state_machine.consume(n);
                            } else {
                                break space;
                            }
                        };
                        let n = file_data.read(space)?;
                        if n == 0 {
                            self.state = State::Flush;
                            continue;
                        }
                        *size = n;
                    } else {
                        let action = self.state_machine.step_write_file_data(file_index, *size)?;
                        match action {
                            WriterAction3::Write => {
                                let data = self.state_machine.data();
                                let n = self.writer.write(data)?;
                                self.state_machine.consume(n);
                            }
                            WriterAction3::Done(written) => {
                                *size -= written;
                            }
                        }
                    }
                }
                State::Flush => {
                    loop {
                        let data = self.state_machine.data();
                        if data.is_empty() {
                            break;
                        }

                        let n = self.writer.write(data)?;
                        self.state_machine.consume(n);
                    }

                    self.state = State::FileData {
                        size: 0,
                        is_new: true,
                    };
                    return Ok(());
                }
            }
        }
    }

    /// Finish writing.
    ///
    /// This is only a convenience function to call the inner [`Write`] object's [`Write::flush`] method.
    pub fn finish(&mut self) -> Result<(), Error> {
        match &mut self.state {
            State::FileHeader => {}
            State::FileData {
                size: 0,
                is_new: true,
            } => {}
            _ => {
                return Err(Error::InvalidState);
            }
        }

        self.writer.flush()?;
        Ok(())
    }
}

#[derive(Debug)]
enum State {
    FileHeader,
    FileData { size: usize, is_new: bool },
    Flush,
}
