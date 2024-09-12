use crate::sans_io::ReaderAction;
use crate::Error;
use std::pin::Pin;
use std::task::ready;
use std::task::Context;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeek;
use tokio::io::AsyncSeekExt;
use tokio::io::ReadBuf;
use tokio::io::SeekFrom;

/// A tokio wrapper for an archive reader.
pub struct TokioReader<R> {
    reader: R,
    state_machine: crate::sans_io::Reader,
}

impl<R> TokioReader<R> {
    /// Make a new [`TokioReader`].
    pub fn new(reader: R) -> Self {
        TokioReader {
            reader,
            state_machine: crate::sans_io::Reader::new(),
        }
    }

    /// Get the inner reader
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Get a mutable ref to the inner reader
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R> TokioReader<R>
where
    R: AsyncRead + AsyncSeek + std::marker::Unpin,
{
    /// Read the header.
    pub async fn read_header(&mut self) -> Result<(), Error> {
        loop {
            match self.state_machine.step_read_header()? {
                ReaderAction::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size]).await?;
                    self.state_machine.fill(n);
                }
                ReaderAction::Done(()) => return Ok(()),
                ReaderAction::Seek(_) => unreachable!(),
            }
        }
    }

    /// Read the next file.
    pub async fn read_file(&mut self) -> Result<Option<File<'_, R>>, Error> {
        loop {
            match self.state_machine.step_read_file_header()? {
                ReaderAction::Read(size) => {
                    let space = self.state_machine.space();
                    let n = self.reader.read(&mut space[..size]).await?;
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
                    self.reader.seek(SeekFrom::Start(position)).await?;
                    self.state_machine.finish_seek();
                }
                ReaderAction::Done(file_header) => {
                    let size = file_header.size;
                    return Ok(Some(File {
                        name: file_header.name,
                        size,
                        reader: &mut self.reader,
                        state_machine: &mut self.state_machine,
                    }));
                }
            }
        }
    }
}

pin_project_lite::pin_project! {
    /// An archive file
    #[derive(Debug)]
    pub struct File<'a, R> {
        name: String,
        size: u32,

        #[pin]
        reader: &'a mut R,
        state_machine: &'a mut crate::sans_io::Reader,
    }
}

impl<R> File<'_, R> {
    /// Get the file path
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Get the file size
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<'a, R> AsyncRead for File<'a, R>
where
    &'a mut R: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut this = self.project();

        loop {
            let action = this
                .state_machine
                .step_read_file_data(buffer.initialize_unfilled())
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

            match action {
                ReaderAction::Read(size) => {
                    let space = this.state_machine.space();
                    let mut space = ReadBuf::new(&mut space[..size]);

                    // Even if we read shorter than requested,
                    // the state machine is tolerant to this
                    // and will request another read if needed.
                    ready!(this.reader.as_mut().poll_read(cx, &mut space))?;

                    let n = space.filled().len();
                    this.state_machine.fill(n);
                }
                ReaderAction::Seek(_) => unreachable!(),
                ReaderAction::Done(n) => {
                    buffer.advance(n);
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}
