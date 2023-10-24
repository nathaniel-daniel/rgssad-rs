use super::AsyncRead2Read;
use super::AsyncWrite2Write;
use crate::Error;
use crate::Writer;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

/// A tokio wrapper for a Writer
pub struct TokioWriter<W> {
    writer: Writer<AsyncWrite2Write<W>>,
}

impl<W> TokioWriter<W> {
    /// Create a new [`TokioWriter`].
    pub fn new(writer: W) -> Self {
        Self {
            writer: Writer::new(AsyncWrite2Write::new(writer)),
        }
    }
}

impl<W> TokioWriter<W>
where
    W: AsyncWrite + Unpin,
{
    /// Write the header.
    pub fn write_header(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        std::future::poll_fn(|cx| {
            let adapter = self.writer.get_mut();
            adapter.set_waker(cx.waker());

            match self.writer.write_header() {
                Ok(()) => Poll::Ready(Ok(())),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Poll::Pending
                }
                Err(error) => Poll::Ready(Err(error)),
            }
        })
    }

    /// Write an entry.
    pub fn write_entry<'a, R>(
        &'a mut self,
        file_name: &'a str,
        file_len: u32,
        mut reader: R,
    ) -> impl Future<Output = Result<(), Error>> + 'a
    where
        R: AsyncRead + Unpin + 'a,
    {
        std::future::poll_fn(move |cx| {
            let adapter = self.writer.get_mut();
            adapter.set_waker(cx.waker());

            let mut read_adapter = AsyncRead2Read::new(&mut reader);
            read_adapter.set_waker(cx.waker());

            match self.writer.write_entry(file_name, file_len, read_adapter) {
                Ok(()) => Poll::Ready(Ok(())),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Poll::Pending
                }
                Err(error) => Poll::Ready(Err(error)),
            }
        })
    }

    /// Finish writing.
    pub fn finish(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        let mut need_shutdown = false;
        std::future::poll_fn(move |cx| {
            if !need_shutdown {
                let adapter = self.writer.get_mut();

                adapter.set_waker(cx.waker());
                match self.writer.finish() {
                    Ok(()) => {
                        need_shutdown = true;
                    }
                    Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        return Poll::Pending;
                    }
                    Err(error) => {
                        return Poll::Ready(Err(error));
                    }
                }
            }

            let async_writer = Pin::new(self.writer.get_mut().get_mut());
            match async_writer.poll_shutdown(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(error)) => Poll::Ready(Err(Error::Io(error))),
                Poll::Pending => Poll::Pending,
            }
        })
    }
}
