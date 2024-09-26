const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;

/// A sans-io writer state machine.
#[derive(Debug)]
pub struct Writer3 {
    buffer: oval::Buffer,
    key: u32,
}

impl Writer3 {
    /// Create a new writer state machine.
    pub fn new(key: u32) -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),
            key,
        }
    }

    /// Get a reference to the output data buffer where data should be taken from.
    ///
    /// The amount of data copied should be marked with [`consume`].
    pub fn data(&self) -> &[u8] {
        self.buffer.data()
    }

    /// Consume a number of bytes from the output buffer.
    pub fn consume(&mut self, size: usize) {
        self.buffer.consume(size);
    }
}
