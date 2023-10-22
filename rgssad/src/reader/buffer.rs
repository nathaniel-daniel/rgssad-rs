use std::io::Read;

/// A utility buffer.
///
/// Used to track what has been written, a `read_exact` impl with guarantees that future calls make progress.
#[derive(Debug)]
pub(super) struct Buffer<B> {
    buffer: B,
    position: usize,
}

impl<B> Buffer<B> {
    /// Make a new empty buffer.
    pub(super) fn new(buffer: B) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    /// Get a ref to the buffer
    pub fn buffer_ref(&self) -> &B {
        &self.buffer
    }

    /// Get a mut ref to the buffer
    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.buffer
    }

    /// Get the current position
    pub fn position(&self) -> usize {
        self.position
    }
}

impl<B> Buffer<B>
where
    B: AsMut<[u8]>,
{
    /// Fill this buffer.
    ///
    /// Errors can be retried without losing data.
    pub(crate) fn fill<R>(&mut self, mut reader: R) -> std::io::Result<()>
    where
        R: Read,
    {
        loop {
            let buffer = self.buffer.as_mut();

            if self.position == buffer.len() {
                return Ok(());
            }

            match reader.read(&mut buffer[self.position..]) {
                Ok(0) => {
                    // We exit above if we have enough data.
                    // Therefore, this is an unexpected EOF.
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ));
                }
                Ok(n) => {
                    self.position += n;
                }
                Err(ref error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(error);
                }
            }
        }
    }
}
