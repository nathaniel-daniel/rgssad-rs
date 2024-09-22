use super::Error;
use super::FileHeader;
use super::ReaderAction;
use crate::crypt_file_data;
use crate::crypt_name_bytes;
use crate::crypt_u32;
use crate::DEFAULT_KEY;
use crate::HEADER_LEN;
use crate::MAGIC;
use crate::MAGIC_LEN_USIZE;
use crate::MAX_FILE_NAME_LEN;
use crate::U32_LEN;
use crate::VERSION;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;

/// A sans-io reader state machine.
#[derive(Debug)]
pub struct Reader {
    buffer: oval::Buffer,

    state: State,
    need_seek: bool,
    position: u64,
    next_file_position: u64,
    key: u32,
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

    /// Step the state machine, performing the action of reading and validating the header.
    ///
    /// If the header has already been read, `Ok(ReaderAction::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically read the header is if has not been read.
    /// This will never request a seek.
    pub fn step_read_header(&mut self) -> Result<ReaderAction<()>, Error> {
        match self.state {
            State::Header => {}
            State::FileHeader | State::FileData { .. } => {
                return Ok(ReaderAction::Done(()));
            }
        }

        let data = self.buffer.data();

        let data_len = data.len();
        if data_len < HEADER_LEN {
            return Ok(ReaderAction::Read(HEADER_LEN - data_len));
        }

        // We validate the size above.
        let magic = *data.first_chunk::<MAGIC_LEN_USIZE>().unwrap();
        if magic != MAGIC {
            return Err(Error::InvalidMagic { magic });
        }

        let version = data[MAGIC_LEN_USIZE];
        if version != VERSION {
            return Err(Error::InvalidVersion { version });
        }

        // We know the header len can fit in a u64.
        let header_len_u64 = u64::try_from(HEADER_LEN).unwrap();
        self.buffer.consume(HEADER_LEN);
        self.position = header_len_u64;
        self.next_file_position = header_len_u64;
        self.state = State::FileHeader;

        Ok(ReaderAction::Done(()))
    }

    /// Step the state machine, performing the action of reading the next file header.
    ///
    /// This will read the header if it has not been read already.
    /// This may request a seek.
    /// If you want to skip over the file data, call this again after it returns `ReaderAction::Done`.
    pub fn step_read_file_header(&mut self) -> Result<ReaderAction<FileHeader>, Error> {
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
                        return Ok(ReaderAction::Seek(self.next_file_position));
                    }

                    self.state = State::FileHeader;
                }
            }
        }

        let mut key = self.key;
        let data = self.buffer.data();
        let data_len = data.len();

        if data_len < U32_LEN {
            return Ok(ReaderAction::Read(U32_LEN - data_len));
        }

        let file_name_len = {
            // We check the buffer size above.
            let bytes = data[..U32_LEN].try_into().unwrap();
            let n = u32::from_le_bytes(bytes);
            let n = crypt_u32(&mut key, n);
            if n > MAX_FILE_NAME_LEN {
                return Err(Error::FileNameTooLongU32 { len: n });
            }

            // We check the file name len above.
            usize::try_from(n).unwrap()
        };

        let file_header_size = (U32_LEN * 2) + file_name_len;
        if data_len < file_header_size {
            return Ok(ReaderAction::Read(file_header_size - data_len));
        }

        let file_name = {
            let mut bytes = data[U32_LEN..U32_LEN + file_name_len].to_vec();
            crypt_name_bytes(&mut key, &mut bytes);

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
            crypt_u32(&mut key, n)
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

        Ok(ReaderAction::Done(FileHeader {
            name: file_name,
            size: file_data_len,
        }))
    }

    /// Read file data.
    ///
    /// This will read the header if it has not already been read.
    /// This will return `Ok(ReaderAction::Done(0))` if a file header has not been read.
    /// This will never request a seek.
    pub fn step_read_file_data(
        &mut self,
        output_buffer: &mut [u8],
    ) -> Result<ReaderAction<usize>, Error> {
        let (key, counter, remaining) = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_read_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader => return Ok(ReaderAction::Done(0)),
                State::FileData {
                    key,
                    counter,
                    remaining,
                } => break (key, counter, remaining),
            }
        };

        if *remaining == 0 {
            return Ok(ReaderAction::Done(0));
        }

        let data = self.buffer.data();
        if data.is_empty() {
            return Ok(ReaderAction::Read(self.buffer.available_space()));
        }

        let remaining_usize =
            usize::try_from(*remaining).expect("remaining bytes cannot fit in a `usize`");
        let len = std::cmp::min(data.len(), output_buffer.len());
        let len = std::cmp::min(len, remaining_usize);
        let len_u32 = u32::try_from(len).expect("len cannot fit in a `u32`");
        let output_buffer = &mut output_buffer[..len];

        output_buffer.copy_from_slice(&data[..len]);
        crypt_file_data(key, counter, output_buffer);
        *remaining -= len_u32;
        self.buffer.consume(len);
        self.position += u64::from(len_u32);

        Ok(ReaderAction::Done(len))
    }
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

/// The parse state
#[derive(Debug, Copy, Clone)]
enum State {
    Header,
    FileHeader,
    FileData {
        key: u32,
        counter: u8,
        remaining: u32,
    },
}
