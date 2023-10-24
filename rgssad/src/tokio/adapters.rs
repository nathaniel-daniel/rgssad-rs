use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use tokio::io::AsyncRead;
use tokio::io::AsyncSeek;
use tokio::io::ReadBuf;

pub(super) struct AsyncRead2Read<R> {
    reader: R,
    waker: Option<Waker>,
}

impl<R> AsyncRead2Read<R> {
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader,
            waker: None,
        }
    }

    pub(super) fn into_inner(self) -> R {
        self.reader
    }

    pub(super) fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    pub(super) fn set_waker(&mut self, new_waker: &Waker) {
        if self
            .waker
            .as_ref()
            .map_or(false, |waker| waker.will_wake(new_waker))
        {
            return;
        }

        self.waker = Some(new_waker.clone());
    }
}

impl<R> std::io::Read for AsyncRead2Read<R>
where
    R: AsyncRead + Unpin,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let reader = Pin::new(&mut self.reader);
        let waker = self
            .waker
            .as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "missing waker"))?;
        let mut cx = Context::from_waker(waker);

        let mut buf = ReadBuf::new(buf);
        match reader.poll_read(&mut cx, &mut buf) {
            Poll::Ready(result) => result.map(|()| buf.filled().len()),
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

impl<R> std::io::Seek for AsyncRead2Read<R>
where
    R: AsyncSeek + Unpin,
{
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let mut reader = Pin::new(&mut self.reader);
        let waker = self
            .waker
            .as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "missing waker"))?;
        let mut cx = Context::from_waker(waker);

        reader.as_mut().start_seek(pos)?;
        match reader.poll_complete(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}
