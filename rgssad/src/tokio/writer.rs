use crate::sans_io::WriterAction;
use crate::Error;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
enum State {
    FileHeader,
    FileData { size: usize },
    Flush,
}

/// The archive writer.
#[derive(Debug)]
pub struct TokioWriter<W> {
    /// The inner writer.
    writer: W,

    /// The current state
    state: State,

    /// The state machine
    state_machine: crate::sans_io::Writer,
}

impl<W> TokioWriter<W> {
    /// Create an archive writer around a writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            state: State::FileHeader,
            state_machine: crate::sans_io::Writer::new(),
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

impl<W> TokioWriter<W>
where
    W: AsyncWrite + Unpin,
{
    /// Write the archive header.
    ///
    /// If the header has already been written, this is a NOP.
    pub async fn write_header(&mut self) -> Result<(), Error> {
        loop {
            let action = self.state_machine.step_write_header()?;
            match action {
                WriterAction::Write => {
                    let data = self.state_machine.data();
                    let size = self.writer.write(data).await?;
                    self.state_machine.consume(size);
                }
                WriterAction::Done(()) => {
                    loop {
                        let data = self.state_machine.data();
                        if data.is_empty() {
                            break;
                        }

                        let n = self.writer.write(data).await?;
                        self.state_machine.consume(n);
                    }

                    self.state = State::FileHeader;
                    return Ok(());
                }
            }
        }
    }

    /// Write a file.
    ///
    /// An file is composed of a name (path), size, and data.
    /// This function may be retried.
    /// To retry, call this function with the same arguments.
    /// Note that if anything other than an I/O error occurs, the written bytes are likely corrupted.
    pub async fn write_file<R>(
        &mut self,
        file_name: &str,
        file_size: u32,
        mut file_data: R,
    ) -> Result<(), Error>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            match &mut self.state {
                State::FileHeader => {
                    let action = self
                        .state_machine
                        .step_write_file_header(file_name, file_size)?;

                    match action {
                        WriterAction::Write => {
                            let data = self.state_machine.data();
                            let size = self.writer.write(data).await?;
                            self.state_machine.consume(size);
                        }
                        WriterAction::Done(()) => {
                            self.state = State::FileData { size: 0 };
                        }
                    }
                }
                State::FileData { size } => {
                    if *size == 0 {
                        let space = loop {
                            let space = self.state_machine.space();
                            if space.is_empty() {
                                let data = self.state_machine.data();
                                let n = self.writer.write(data).await?;
                                self.state_machine.consume(n);
                            } else {
                                break space;
                            }
                        };
                        let n = file_data.read(space).await?;
                        if n == 0 {
                            self.state = State::Flush;
                            continue;
                        }
                        *size = n;
                    } else {
                        let action = self.state_machine.step_write_file_data(*size)?;
                        match action {
                            WriterAction::Write => {
                                let data = self.state_machine.data();
                                let n = self.writer.write(data).await?;
                                self.state_machine.consume(n);
                            }
                            WriterAction::Done(written) => {
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

                        let n = self.writer.write(data).await?;
                        self.state_machine.consume(n);
                    }

                    self.state = State::FileHeader;
                    return Ok(());
                }
            }
        }
    }

    /// Finish writing.
    ///
    /// This is only a convenience function to call the inner [`Write`] object's [`Write::flush`] method.
    pub async fn finish(&mut self) -> Result<(), Error> {
        match &mut self.state {
            State::FileHeader => {}
            _ => {
                return Err(Error::InvalidState);
            }
        }

        self.writer.flush().await?;
        Ok(())
    }
}
