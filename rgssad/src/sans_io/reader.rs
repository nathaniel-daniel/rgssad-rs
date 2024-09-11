use super::Error;
use super::SansIoAction;
use crate::MAGIC;
use crate::MAGIC_LEN;
use crate::VERSION;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;
const HEADER_LEN: usize = MAGIC_LEN + 1;

/// A sans-io reader state machine.
#[derive(Debug)]
pub struct Reader {
    buffer: oval::Buffer,

    read_header: bool,
    position: u64,
    is_eof: bool,
}

impl Reader {
    /// Create a new reader state machine.
    pub fn new() -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),

            read_header: false,
            position: 0,
            is_eof: false,
        }
    }

    /// Get a reference to the read buffer part where new data should be written.
    ///
    /// You should indicate how many bytes were written with `fill`.
    pub fn space(&mut self) -> &mut [u8] {
        self.buffer.space()
    }

    pub fn fill(&mut self, num: usize) {
        self.buffer.fill(num);
    }

    /// Let the state machine know if it is at the end of the file or not.
    ///
    /// This is needed to know the difference between the end of a file and a truncated file.
    pub fn set_eof(&mut self, is_eof: bool) {
        self.is_eof = is_eof;
    }

    /// Step the state machine, performing the action of reading and validating the header.
    ///
    /// If the header has already been read, `Ok(SansIoAction::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically read the header is if has not been read.
    /// This will never request a seek.
    pub fn step_read_header(&mut self) -> Result<SansIoAction<()>, Error> {
        if self.read_header {
            return Ok(SansIoAction::Done(()));
        }

        let data = self.buffer.data();

        let data_len = data.len();
        if data_len < HEADER_LEN {
            if self.is_eof {
                return Err(Error::UnexpectedEof);
            }

            return Ok(SansIoAction::Read(HEADER_LEN - data_len));
        }

        // We validate the size above.
        let magic = *data.first_chunk::<MAGIC_LEN>().unwrap();
        if magic != MAGIC {
            return Err(Error::InvalidMagic { magic });
        }

        let version = data[MAGIC_LEN];
        if version != VERSION {
            return Err(Error::InvalidVersion { version });
        }

        self.buffer.consume(HEADER_LEN);
        // We know the header len can fit in a u64.
        self.position += u64::try_from(HEADER_LEN).unwrap();
        self.read_header = true;

        Ok(SansIoAction::Done(()))
    }

    /// Step the state machine, performing the action of reading the next file entry header.
    ///
    /// This will read the header if it has not been read already.
    pub fn step_read_entry_header(&mut self) -> Result<SansIoAction<()>, Error> {
        if !self.read_header {
            let action = self.step_read_header()?;
            if !action.is_done() {
                return Ok(action);
            }
        }

        todo!()
    }
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}
