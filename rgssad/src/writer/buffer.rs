use super::write_all;
use std::io::Write;

#[derive(Debug)]
pub(super) struct Buffer<B> {
    buffer: B,
    position: usize,
}

impl<B> Buffer<B> {
    /// Make a new buffer.
    pub(super) fn new(buffer: B) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }
}

impl<B> Buffer<B>
where
    B: AsRef<[u8]>,
{
    /// Write from this buffer to a writer.
    ///
    /// This remembers how many bytes were written so that it may be retried on error without data loss.
    pub(super) fn write<W>(&mut self, writer: W) -> std::io::Result<()>
    where
        W: Write,
    {
        write_all(writer, self.buffer.as_ref(), &mut self.position)
    }
}
