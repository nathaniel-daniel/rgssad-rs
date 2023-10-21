use crate::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use tokio::io::AsyncBufRead;
use tokio::io::AsyncRead;
use tokio::io::AsyncSeek;
use tokio::io::ReadBuf;

struct ReaderAdapter<R> {
    reader: R,
    waker: Option<Waker>,
}

impl<R> ReaderAdapter<R>
where
    R: AsyncRead,
{
    fn new(reader: R) -> Self {
        Self {
            reader: reader,
            waker: None,
        }
    }

    fn into_inner(self) -> R {
        self.reader
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

impl<R> TokioReader<R>
where
    R: AsyncRead + AsyncSeek,
{
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
}

impl<R> TokioReader<R>
where
    R: AsyncBufRead + AsyncSeek + std::marker::Unpin,
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
    pub fn read_entry(&mut self) -> impl Future<Output = Result<Option<()>, Error>> + '_ {
        std::future::poll_fn(|cx| {
            let adapter = self.reader.get_mut();
            adapter.waker = Some(cx.waker().clone());

            match self.reader.read_entry() {
                Ok(result) => Poll::Ready(Ok(result.map(|_entry| ()))),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Poll::Pending
                }
                Err(error) => Poll::Ready(Err(error)),
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::VX_TEST_GAME;
    use std::io::Seek;
    use std::io::SeekFrom;

    #[tokio::test]
    async fn reader_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        // Ensure skipping works.
        let mut num_skipped_entries = 0;
        while let Some(_entry) = reader.read_entry().await.expect("failed to read entry") {
            num_skipped_entries += 1;
        }

        // Reset position and recreate reader.
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        let mut entries = Vec::new();
        while let Some(entry) = reader.read_entry().await.expect("failed to read entry") {
            /*
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
            */
            entries.push(entry);
        }

        assert!(entries.len() == num_skipped_entries);
    }
}
