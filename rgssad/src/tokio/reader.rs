use crate::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::ready;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeek;
use tokio::io::ReadBuf;

struct ReaderAdapter<R> {
    reader: R,
    waker: Option<Waker>,
}

impl<R> ReaderAdapter<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            waker: None,
        }
    }

    fn into_inner(self) -> R {
        self.reader
    }

    fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R> std::io::Read for ReaderAdapter<R>
where
    R: AsyncRead + Unpin,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let reader = Pin::new(&mut self.reader);
        let waker = self.waker.as_ref().expect("missing waker");
        let mut cx = Context::from_waker(waker);

        let mut buf = ReadBuf::new(buf);
        match reader.poll_read(&mut cx, &mut buf) {
            Poll::Ready(result) => result.map(|()| buf.filled().len()),
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

impl<R> std::io::Seek for ReaderAdapter<R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let mut reader = Pin::new(&mut self.reader);
        let waker = self.waker.as_ref().expect("missing waker");
        let mut cx = Context::from_waker(waker);

        reader.as_mut().start_seek(pos)?;
        match reader.poll_complete(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

/// A tokio wrapper for an archive reader.
pub struct TokioReader<R> {
    reader: crate::Reader<ReaderAdapter<R>>,
}

impl<R> TokioReader<R> {
    /// Make a new [`TokioReader`].
    pub fn new(reader: R) -> Self {
        Self {
            reader: crate::Reader::new(ReaderAdapter::new(reader)),
        }
    }

    /// Get the inner reader
    pub fn into_inner(self) -> R {
        self.reader.into_inner().into_inner()
    }

    /// Get a mutable ref to the inner reader
    pub fn get_mut(&mut self) -> &mut R {
        self.reader.get_mut().get_mut()
    }
}

impl<R> TokioReader<R>
where
    R: AsyncRead + AsyncSeek + std::marker::Unpin,
{
    /// Read the header.
    pub fn read_header(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        std::future::poll_fn(|cx| {
            let adapter = self.reader.get_mut();
            adapter.waker = Some(cx.waker().clone());

            match self.reader.read_header() {
                Ok(()) => Poll::Ready(Ok(())),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Poll::Pending
                }
                Err(error) => Poll::Ready(Err(error)),
            }
        })
    }

    /// Read the next entry.
    pub fn read_entry(&mut self) -> ReadEntryFuture<'_, R> {
        ReadEntryFuture { reader: Some(self) }
    }
}

/// The future for reading the next [`Entry`].
pub struct ReadEntryFuture<'a, R> {
    reader: Option<&'a mut TokioReader<R>>,
}

impl<'a, R> Future for ReadEntryFuture<'a, R>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    type Output = Result<Option<Entry<'a, R>>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let reader = self.reader.as_mut().expect("missing reader");

        let adapter = reader.reader.get_mut();
        adapter.waker = Some(cx.waker().clone());

        match reader.reader.read_entry() {
            Ok(result) => match result {
                Some(entry) => {
                    let file_name = entry.file_name;
                    let size = entry.size;
                    let key = entry.key;
                    let reader = self.reader.take().expect("missing reader");

                    Poll::Ready(Ok(Some(Entry {
                        file_name,
                        size,
                        key,
                        reader: reader.get_mut().take(size.into()),
                        counter: 0,
                    })))
                }
                None => Poll::Ready(Ok(None)),
            },
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Poll::Pending
            }
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

/// An archive entry
#[derive(Debug)]
pub struct Entry<'a, R> {
    file_name: String,
    size: u32,
    key: u32,
    reader: tokio::io::Take<&'a mut R>,
    counter: u8,
}

impl<R> Entry<'_, R> {
    /// Get the file name
    pub fn file_name(&self) -> &str {
        self.file_name.as_str()
    }

    /// Get the file size
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<'a, R> AsyncRead for Entry<'a, R>
where
    &'a mut R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let initial_filled_len = buf.filled().len();

        let reader = Pin::new(&mut self.reader);
        let result = ready!(reader.poll_read(cx, buf));

        let this = self.get_mut();
        let key = &mut this.key;
        let counter = &mut this.counter;

        crate::reader::decrypt_entry_bytes(
            &mut buf.filled_mut()[initial_filled_len..],
            key,
            counter,
        );

        Poll::Ready(result)
    }
}
