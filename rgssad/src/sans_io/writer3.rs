use super::Error;
use super::WriterAction3;
use crate::HEADER_LEN3_USIZE;
use crate::KEY_LEN_USIZE;
use crate::MAGIC;
use crate::MAGIC_LEN_USIZE;
use crate::VERSION3;
use crate::VERSION_LEN_USIZE;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;

/// A sans-io writer state machine.
#[derive(Debug)]
pub struct Writer3 {
    buffer: oval::Buffer,
    key: u32,
    state: State,
}

impl Writer3 {
    /// Create a new writer state machine.
    pub fn new(key: u32) -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),
            key,
            state: State::Header,
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

    /// Step the state machine, performing the action of writing the header.
    ///
    /// If the header has already been written, `Ok(WriterAction3::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically write the header is if has not been written.
    pub fn step_write_header(&mut self) -> Result<WriterAction3<()>, Error> {
        match self.state {
            State::Header => {}
            State::FileHeader | State::FileData { .. } => {
                return Ok(WriterAction3::Done(()));
            }
        }

        let space = self.buffer.space();
        if space.len() < HEADER_LEN3_USIZE {
            return Ok(WriterAction3::Write);
        }

        // We validate the size above.
        let (magic_space, space) = space.split_first_chunk_mut::<MAGIC_LEN_USIZE>().unwrap();
        magic_space.copy_from_slice(&MAGIC);

        let (version_space, space) = space.split_first_chunk_mut::<VERSION_LEN_USIZE>().unwrap();
        version_space[0] = VERSION3;

        let (key_space, _space) = space.split_first_chunk_mut::<KEY_LEN_USIZE>().unwrap();
        let key = self.key.overflowing_sub(3).0.overflowing_div(9).0;
        key_space.copy_from_slice(&key.to_le_bytes());

        self.buffer.fill(HEADER_LEN3_USIZE);

        self.state = State::FileHeader;

        Ok(WriterAction3::Done(()))
    }
}

#[derive(Debug)]
enum State {
    Header,
    FileHeader,
    FileData {},
}
