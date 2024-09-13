use super::Error;
use super::WriterAction;
use crate::sans_io::FileHeader;
use crate::DEFAULT_KEY;
use crate::HEADER_LEN;
use crate::MAGIC;
use crate::MAGIC_LEN;
use crate::MAX_FILE_NAME_LEN;
use crate::U32_LEN;
use crate::VERSION;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;

/// A sans-io writer state machine.
#[derive(Debug)]
pub struct Writer {
    buffer: oval::Buffer,
    key: u32,
    state: State,
}

impl Writer {
    /// Create a new writer state machine.
    pub fn new() -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),
            key: DEFAULT_KEY,
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
    /// If the header has already been written, `Ok(Writer::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically write the header is if has not been written.
    pub fn step_write_header(&mut self) -> Result<WriterAction<()>, Error> {
        match self.state {
            State::Header => {}
            State::FileHeader | State::FileData => {
                return Ok(WriterAction::Done(()));
            }
        }

        let space = self.buffer.space();
        if space.len() < HEADER_LEN {
            return Ok(WriterAction::Write);
        }

        space[..MAGIC_LEN].copy_from_slice(&MAGIC);
        space[MAGIC_LEN] = VERSION;
        self.buffer.fill(HEADER_LEN);

        self.state = State::FileHeader;

        Ok(WriterAction::Done(()))
    }

    /// Step the state machine, performing the action of writing the next file header.
    ///
    /// This will write the header if it has not been written already.
    pub fn write_file_header(
        &mut self,
        file_header: FileHeader,
    ) -> Result<WriterAction<()>, Error> {
        loop {
            match self.state {
                State::Header => {
                    let action = self.step_write_header()?;
                    if !action.is_done() {
                        return Ok(action);
                    }
                }
                State::FileHeader => {
                    break;
                }
                State::FileData => {
                    todo!("State::FileData");
                }
            }
        }

        let name_len = file_header.name.len();
        if name_len > usize::try_from(MAX_FILE_NAME_LEN).unwrap() {
            return Err(Error::FileNameTooLongUsize { len: name_len });
        }
        // We check the name size above.
        let name_len_u32 = u32::try_from(name_len).unwrap();

        let space = self.buffer.space();
        let needed_size = (U32_LEN * 2) + name_len;
        if space.len() < needed_size {
            return Ok(WriterAction::Write);
        }

        space.copy_from_slice(&name_len_u32.to_le_bytes());

        todo!("write_file_header")
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum State {
    Header,
    FileHeader,
    FileData,
}
