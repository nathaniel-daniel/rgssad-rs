use super::Action;
use super::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::MAGIC_LEN;
use crate::MAX_FILE_NAME_LEN;
use crate::VERSION;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;
const HEADER_LEN: usize = MAGIC_LEN + 1;
const U32_LEN: usize = 4;

/// Decrypt an encrypted u32, and rotate the key as needed.
fn decrypt_u32(key: &mut u32, mut n: u32) -> u32 {
    n ^= *key;
    *key = key.overflowing_mul(7).0.overflowing_add(3).0;
    n
}

/// Decrypt the encrypted file data bytes.
fn decrypt_file_data_bytes(buffer: &mut [u8], key: &mut u32, counter: &mut u8) {
    for byte in buffer.iter_mut() {
        *byte ^= key.to_le_bytes()[usize::from(*counter)];
        if *counter == 3 {
            *key = key.overflowing_mul(7).0.overflowing_add(3).0;
        }
        *counter = (*counter + 1) % 4;
    }
}

/// A sans-io reader state machine.
#[derive(Debug)]
pub struct Reader {
    buffer: oval::Buffer,

    state: State,
    need_seek: bool,
    position: u64,
    next_file_position: u64,
    pub(crate) key: u32,
}

impl Reader {
    /// Create a new reader state machine.
    pub fn new() -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),

            state: State::Header,
            need_seek: false,
            position: 0,
            next_file_position: 0,
            key: DEFAULT_KEY,
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

    /// Get the amount of data currently in the buffer.
    pub fn available_data(&self) -> usize {
        self.buffer.available_data()
    }

    /// Tell the state machine that the seek it requested if finished.
    ///
    /// This will clear any buffered bytes.
    ///
    /// # Panics
    /// This will panic if a seek was not requested.
    pub fn finish_seek(&mut self) {
        assert!(
            self.position != self.next_file_position,
            "a seek was not requested"
        );

        self.position = self.next_file_position;
        self.buffer.reset();
    }

    /// Returns `true` if the header has been read.
    pub fn read_header(&mut self) -> bool {
        self.state != State::Header
    }

    /// Step the state machine, performing the action of reading and validating the header.
    ///
    /// If the header has already been read, `Ok(Action::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically read the header is if has not been read.
    /// This will never request a seek.
    pub fn step_read_header(&mut self) -> Result<Action<()>, Error> {
        if self.state != State::Header {
            return Ok(Action::Done(()));
        }

        let data = self.buffer.data();

        let data_len = data.len();
        if data_len < HEADER_LEN {
            return Ok(Action::Read(HEADER_LEN - data_len));
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

        // We know the header len can fit in a u64.
        let header_len_u64 = u64::try_from(HEADER_LEN).unwrap();
        self.buffer.consume(HEADER_LEN);
        self.position = header_len_u64;
        self.next_file_position = header_len_u64;
        self.state = State::FileHeader;

        Ok(Action::Done(()))
    }

    /// Step the state machine, performing the action of reading the next file header.
    ///
    /// This will read the header if it has not been read already.
    pub fn step_read_file_header(&mut self) -> Result<Action<FileHeader>, Error> {
        loop {
            match self.state {
                State::Header => {
                    let action = self.step_read_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader => break,
                State::FileData { .. } => {
                    if self.position != self.next_file_position {
                        return Ok(Action::Seek(self.next_file_position));
                    }

                    self.state = State::FileHeader;
                }
            }
        }

        let mut key = self.key;
        let data = self.buffer.data();
        let data_len = data.len();

        if data_len < U32_LEN {
            return Ok(Action::Read(U32_LEN - data_len));
        }

        let file_name_len = {
            // We check the buffer size above.
            let bytes = data[..U32_LEN].try_into().unwrap();
            let n = u32::from_le_bytes(bytes);
            let n = decrypt_u32(&mut key, n);
            if n > MAX_FILE_NAME_LEN {
                return Err(Error::FileNameTooLong { len: n });
            }

            // We check the file name len above.
            usize::try_from(n).unwrap()
        };

        let file_header_size = (U32_LEN * 2) + file_name_len;
        if data_len < file_header_size {
            return Ok(Action::Read(file_header_size - data_len));
        }

        let file_name = {
            let mut bytes = data[U32_LEN..U32_LEN + file_name_len].to_vec();
            for byte in bytes.iter_mut() {
                // We mask with 0xFF, this cannot exceed the bounds of a u8.
                *byte ^= u8::try_from(key & 0xFF).unwrap();
                key = key.overflowing_mul(7).0.overflowing_add(3).0;
            }

            // I'm fairly certain these are required to be ASCII, but I forget the source.
            //
            // TODO:
            // Link source for ASCII file names, or do not assume ASCII file names.
            String::from_utf8(bytes).map_err(|error| Error::InvalidFileName { error })?
        };

        let file_data_len = {
            let index = U32_LEN + file_name_len;
            let range = index..index + U32_LEN;
            let bytes = data[range].try_into().unwrap();
            let n = u32::from_le_bytes(bytes);
            decrypt_u32(&mut key, n)
        };

        // This should not be able to overflow a u64.
        let file_header_size_u64 = u64::try_from(file_header_size).unwrap();
        self.buffer.consume(file_header_size);
        self.position += file_header_size_u64;
        // Calculate the offset of the next file:
        // size_of(file_name_len) + size_of(file_name) + size_of(file_data_len) + size_of(file_data)
        self.next_file_position += file_header_size_u64 + u64::from(file_data_len);
        self.key = key;
        self.need_seek = true;
        self.state = State::FileData {
            key: self.key,
            counter: 0,
            remaining: file_data_len,
        };

        Ok(Action::Done(FileHeader {
            name: file_name,
            size: file_data_len,
        }))
    }

    /// Read file data.
    ///
    /// This will never request a seek.
    pub fn step_read_file_data(
        &mut self,
        output_buffer: &mut [u8],
    ) -> Result<Action<usize>, Error> {
        let (key, counter, remaining) = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_read_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader => return Ok(Action::Done(0)),
                State::FileData {
                    key,
                    counter,
                    remaining,
                } => break (key, counter, remaining),
            }
        };

        if *remaining == 0 {
            return Ok(Action::Done(0));
        }

        let data = self.buffer.data();
        if data.is_empty() {
            return Ok(Action::Read(self.buffer.available_space()));
        }

        let remaining_usize =
            usize::try_from(*remaining).expect("remaining bytes cannot fit in a `usize`");
        let len = std::cmp::min(data.len(), output_buffer.len());
        let len = std::cmp::min(len, remaining_usize);
        let len_u32 = u32::try_from(len).expect("len cannot fit in a `u32`");
        let output_buffer = &mut output_buffer[..len];

        output_buffer.copy_from_slice(&data[..len]);
        decrypt_file_data_bytes(output_buffer, key, counter);
        *remaining -= len_u32;
        self.buffer.consume(len);
        self.position += u64::from(len_u32);

        Ok(Action::Done(len))
    }
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

/// A file header
#[derive(Debug)]
pub struct FileHeader {
    /// The file name
    pub name: String,

    /// The file data size.
    pub size: u32,
}

/// The parse state
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum State {
    Header,
    FileHeader,
    FileData {
        key: u32,
        counter: u8,
        remaining: u32,
    },
}
