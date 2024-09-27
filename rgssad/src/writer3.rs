use crate::sans_io::WriterAction3;
use crate::Error;
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

    /// Finish writing.
    ///
    /// This is only a convenience function to call the inner [`Write`] object's [`Write::flush`] method.
    pub fn finish(&mut self) -> Result<(), Error> {
        match &mut self.state {
            State::FileHeader => {}
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
    FileData { size: usize },
    Flush,
}
