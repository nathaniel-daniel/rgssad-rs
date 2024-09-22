use super::Error;
use super::ReaderAction3;
use crate::MAGIC;
use crate::MAGIC_LEN;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;
const HEADER_LEN3: usize = 8;

/// A sans-io reader state machine.
#[derive(Debug)]
pub struct Reader3 {
    buffer: oval::Buffer,

    state: State,
}

impl Reader3 {
    /// Create a new reader state machine.
    pub fn new() -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),

            state: State::Header,
        }
    }

    /// Get a reference to the read buffer part where new data should be written.
    ///
    /// You should indicate how many bytes were written with `fill`.
    pub fn space(&mut self) -> &mut [u8] {
        self.buffer.space()
    }

    /// Set the number of bytes written to the space buffer.
    pub fn fill(&mut self, num: usize) {
        self.buffer.fill(num);
    }

    /// Step the state machine, performing the action of reading and validating the header.
    ///
    /// If the header has already been read, `Ok(ReaderAction3::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically read the header is if has not been read.
    /// This will never request a seek.
    pub fn step_read_header(&mut self) -> Result<ReaderAction3<()>, Error> {
        match self.state {
            State::Header => {}
            State::FileHeader | State::FileData => {
                return Ok(ReaderAction3::Done(()));
            }
        }

        let data = self.buffer.data();

        let data_len = data.len();
        if data_len < HEADER_LEN3 {
            return Ok(ReaderAction3::Read(HEADER_LEN3 - data_len));
        }

        // We validate the size above.
        let (magic, data) = data.split_first_chunk::<MAGIC_LEN>().unwrap();
        if *magic != MAGIC {
            return Err(Error::InvalidMagic { magic: *magic });
        }

        Ok(ReaderAction3::Done(()))
    }
}

impl Default for Reader3 {
    fn default() -> Self {
        Self::new()
    }
}

/// The parse state
#[derive(Debug, Copy, Clone)]
enum State {
    Header,
    FileHeader,
    FileData,
}
