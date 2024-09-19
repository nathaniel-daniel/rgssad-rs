use super::Error;
use super::WriterAction;
use crate::crypt_file_data;
use crate::crypt_name_bytes;
use crate::crypt_u32;
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
    pub(crate) key: u32,
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

    /// Get a reference to the space data buffer where data should inserted.
    pub fn space(&mut self) -> &mut [u8] {
        self.buffer.space()
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
            State::FileHeader | State::FileData { .. } => {
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
    pub fn step_write_file_header(
        &mut self,
        name: &str,
        size: u32,
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
                State::FileData { remaining, .. } => {
                    todo!("write_file_header -> State::FileData, remaining={remaining}");
                }
            }
        }

        let name_len = name.len();
        if name_len > usize::try_from(MAX_FILE_NAME_LEN).unwrap() {
            return Err(Error::FileNameTooLongUsize { len: name_len });
        }
        // We check the name size above.
        let name_len_u32 = u32::try_from(name_len).unwrap();

        let space = self.buffer.space();
        let file_header_size = (U32_LEN * 2) + name_len;
        if space.len() < file_header_size {
            return Ok(WriterAction::Write);
        }

        let mut key = self.key;

        let data = crypt_u32(&mut key, name_len_u32);
        let (bytes, space) = space.split_at_mut(U32_LEN);
        bytes.copy_from_slice(&data.to_le_bytes());

        let (bytes, space) = space.split_at_mut(name_len);
        bytes.copy_from_slice(name.as_bytes());
        crypt_name_bytes(&mut key, bytes);

        let data = crypt_u32(&mut key, size);
        let (bytes, _space) = space.split_at_mut(U32_LEN);
        bytes.copy_from_slice(&data.to_le_bytes());

        self.key = key;
        self.buffer.fill(file_header_size);

        self.state = State::FileData {
            key,
            counter: 0,
            remaining: size,
        };

        Ok(WriterAction::Done(()))
    }

    /// Step the state machine, performing the action of writing the file data.
    ///
    /// This will write the header if it has not been written already.
    /// Populate the space buffer with the bytes to write, then pass the number of bytes written to this function.
    pub fn step_write_file_data(&mut self, size: usize) -> Result<WriterAction<usize>, Error> {
        let (key, counter, remaining) = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_write_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader => {
                    todo!("step_write_file_data -> State::FileHeader");
                }
                State::FileData {
                    key,
                    counter,
                    remaining,
                } => {
                    break (key, counter, remaining);
                }
            }
        };

        let space = self.buffer.space();
        crypt_file_data(key, counter, &mut space[..size]);

        *remaining -= u32::try_from(size).expect("number of bytes written cannot fit in a `u32`");
        if *remaining == 0 {
            self.state = State::FileHeader;
        }

        self.buffer.fill(size);

        Ok(WriterAction::Done(size))
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
    FileData {
        key: u32,
        counter: u8,
        remaining: u32,
    },
}
