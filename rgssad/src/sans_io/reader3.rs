use super::Error;
use super::FileHeader3;
use super::ReaderAction3;
use crate::crypt_file_data;
use crate::crypt_name_bytes3;
use crate::HEADER_LEN3;
use crate::HEADER_LEN3_USIZE;
use crate::KEY_LEN_USIZE;
use crate::MAGIC;
use crate::MAGIC_LEN_USIZE;
use crate::U32_LEN;
use crate::VERSION3;
use crate::VERSION_LEN_USIZE;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;
const HEADER_LEN3_U64: u64 = HEADER_LEN3 as u64;

/// A sans-io reader state machine.
#[derive(Debug)]
pub struct Reader3 {
    buffer: oval::Buffer,

    state: State,
    position: u64,
    key: Option<u32>,
    next_file_header_position: Option<u64>,
}

impl Reader3 {
    /// Create a new reader state machine.
    pub fn new() -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),

            state: State::Header,
            position: 0,
            key: None,
            next_file_header_position: Some(HEADER_LEN3_U64),
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

    /// Tell the state machine that a seek was completed,
    /// and that the file position has been updated.
    ///
    /// This will clear any buffered bytes.
    pub fn finish_seek(&mut self, position: u64) {
        self.position = position;
        self.buffer.reset();
    }

    /// Get the key.
    ///
    /// # Returns
    /// This will return `None` if the header has not been read.
    pub fn key(&self) -> Option<u32> {
        self.key
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
            State::FileHeader { .. } | State::FileData { .. } => {
                return Ok(ReaderAction3::Done(()));
            }
        }

        let data = self.buffer.data();

        let data_len = data.len();
        if data_len < HEADER_LEN3_USIZE {
            return Ok(ReaderAction3::Read(HEADER_LEN3_USIZE - data_len));
        }

        // We validate the size above.
        let (magic, data) = data.split_first_chunk::<MAGIC_LEN_USIZE>().unwrap();
        if *magic != MAGIC {
            return Err(Error::InvalidMagic { magic: *magic });
        }

        let (version, data) = data.split_first_chunk::<VERSION_LEN_USIZE>().unwrap();
        let version = version[0];
        if version != VERSION3 {
            return Err(Error::InvalidVersion { version });
        }

        let (key, _data) = data.split_first_chunk::<KEY_LEN_USIZE>().unwrap();
        let mut key = u32::from_le_bytes(*key);
        key = key.overflowing_mul(9).0.overflowing_add(3).0;

        self.buffer.consume(HEADER_LEN3_USIZE);
        self.position = HEADER_LEN3_U64;
        self.key = Some(key);
        self.state = State::FileHeader;

        Ok(ReaderAction3::Done(()))
    }

    /// Step the state machine, performing the action of reading the next file header.
    ///
    /// This will read the header if it has not been read already.
    /// This may request a seek.
    pub fn step_read_file_header(&mut self) -> Result<ReaderAction3<Option<FileHeader3>>, Error> {
        // We may make the logic here more complicated later.
        #[expect(clippy::while_let_loop)]
        loop {
            match self.state {
                State::Header => {
                    let action = self.step_read_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader | State::FileData { .. } => break,
            }
        }
        // The key will be always be `Some` here, since we must have read the header by now.
        let key = self.key.unwrap();

        // Seek if needed.
        // This is required since we may have seeked and read some file data.
        match self.next_file_header_position {
            Some(next_file_header_position) => {
                if self.position != next_file_header_position {
                    return Ok(ReaderAction3::Seek(next_file_header_position));
                }
            }
            None => {
                return Ok(ReaderAction3::Done(None));
            }
        }

        let data = self.buffer.data();

        let data_len = data.len();
        let mut expected_size = 4 * U32_LEN;
        if data_len < expected_size {
            return Ok(ReaderAction3::Read(expected_size - data_len));
        }

        let (offset, data) = data.split_first_chunk::<U32_LEN>().unwrap();
        let mut offset = u32::from_le_bytes(*offset);
        offset ^= key;

        let (size, data) = data.split_first_chunk::<U32_LEN>().unwrap();
        let mut size = u32::from_le_bytes(*size);
        size ^= key;

        let (file_key, data) = data.split_first_chunk::<U32_LEN>().unwrap();
        let mut file_key = u32::from_le_bytes(*file_key);
        file_key ^= key;

        let (name_len, data) = data.split_first_chunk::<U32_LEN>().unwrap();
        let mut name_len = u32::from_le_bytes(*name_len);
        name_len ^= key;

        if offset == 0 {
            let expected_size_u64 =
                u64::try_from(expected_size).expect("expected_size cannot fit in a `u64`");
            self.buffer.consume(expected_size);
            self.position += expected_size_u64;

            self.next_file_header_position = None;
            return Ok(ReaderAction3::Done(None));
        }

        let name_len_usize = usize::try_from(name_len).expect("name size cannot fit in a `usize`");
        expected_size += name_len_usize;
        if data_len < expected_size {
            return Ok(ReaderAction3::Read(expected_size - data_len));
        }

        let mut name_bytes = data[..name_len_usize].to_vec();
        crypt_name_bytes3(key, &mut name_bytes);
        let name =
            String::from_utf8(name_bytes).map_err(|error| Error::InvalidFileName { error })?;

        let expected_size_u64 =
            u64::try_from(expected_size).expect("expected_size cannot fit in a `u64`");
        self.buffer.consume(expected_size);
        self.position += expected_size_u64;

        self.next_file_header_position = Some(self.position);
        Ok(ReaderAction3::Done(Some(FileHeader3 {
            name,
            size,
            key: file_key,
            offset,
        })))
    }

    /// Read file data.
    ///
    /// This will read the header if it has not already been read.
    /// This may request a seek.
    pub fn step_read_file_data(
        &mut self,
        file_header: &FileHeader3,
        output_buffer: &mut [u8],
    ) -> Result<ReaderAction3<usize>, Error> {
        let offset_u64 = u64::from(file_header.offset);

        let key = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_read_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader { .. } => {
                    if self.position != offset_u64 {
                        return Ok(ReaderAction3::Seek(offset_u64));
                    }

                    self.state = State::FileData {
                        offset: file_header.offset,
                        key: file_header.key,
                    };
                }
                State::FileData { offset, key } => {
                    // The user changed what file they were reading.
                    // Reset the file read state.
                    if file_header.offset != *offset {
                        // Seek to the new file.
                        if self.position != offset_u64 {
                            return Ok(ReaderAction3::Seek(offset_u64));
                        }

                        self.state = State::FileData {
                            offset: file_header.offset,
                            key: file_header.key,
                        };
                        continue;
                    }

                    break key;
                }
            }
        };

        let remaining = {
            let relative_position = self.position.saturating_sub(offset_u64);
            u64::from(file_header.size).saturating_sub(relative_position)
        };
        let remaining_usize =
            usize::try_from(remaining).expect("remaining cannot fit in a `usize`");

        if remaining == 0 {
            return Ok(ReaderAction3::Done(0));
        }

        let data = self.buffer.data();
        if data.is_empty() {
            let len = std::cmp::min(remaining_usize, self.buffer.available_space());
            return Ok(ReaderAction3::Read(len));
        }

        let len = std::cmp::min(data.len(), output_buffer.len());
        let len = std::cmp::min(len, remaining_usize);
        let len_u32 = u32::try_from(len).expect("len cannot fit in a `u32`");
        let output_buffer = &mut output_buffer[..len];

        output_buffer.copy_from_slice(&data[..len]);
        let mut counter = u8::try_from(self.position.saturating_sub(offset_u64) % 4).unwrap();
        crypt_file_data(key, &mut counter, output_buffer);
        self.buffer.consume(len);
        self.position += u64::from(len_u32);

        Ok(ReaderAction3::Done(len))
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
    FileData { offset: u32, key: u32 },
}
