use super::Error;
use super::WriterAction3;
use crate::crypt_file_data;
use crate::crypt_name_bytes3;
use crate::HEADER_LEN3_U32;
use crate::HEADER_LEN3_USIZE;
use crate::KEY_LEN_USIZE;
use crate::MAGIC;
use crate::MAGIC_LEN_USIZE;
use crate::MAX_FILE_NAME_LEN_USIZE;
use crate::U32_LEN;
use crate::VERSION3;
use crate::VERSION_LEN_USIZE;

const DEFAULT_BUFFER_CAPACITY: usize = 10 * 1024;

/// A sans-io writer state machine.
#[derive(Debug)]
pub struct Writer3 {
    buffer: oval::Buffer,
    key: u32,
    state: State,

    files: Vec<(String, u32, u32)>,
    data_position: u32,
}

impl Writer3 {
    /// Create a new writer state machine.
    pub fn new(key: u32) -> Self {
        Self {
            buffer: oval::Buffer::with_capacity(DEFAULT_BUFFER_CAPACITY),
            key,
            state: State::Header,

            files: Vec::new(),
            // sizeof(header) + sizeof(end file header)
            data_position: HEADER_LEN3_U32 + u32::try_from(U32_LEN * 4).unwrap(),
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

    /// Get a reference to the space data buffer where data should inserted.
    pub fn space(&mut self) -> &mut [u8] {
        self.buffer.space()
    }

    /// Step the state machine, performing the action of writing the header.
    ///
    /// If the header has already been written, `Ok(WriterAction3::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically write the header is if has not been written.
    pub fn step_write_header(&mut self) -> Result<WriterAction3<()>, Error> {
        match self.state {
            State::Header => {}
            State::FileHeader { .. } | State::FileData { .. } => {
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

        self.state = State::FileHeader {
            index: 0,
            relative_data_offset: 0,
        };

        Ok(WriterAction3::Done(()))
    }

    /// Add a file.
    ///
    /// This only tells the writer about the file.
    /// Writing the file metadata and data is done separately.
    /// This can only be called before file header writing begins.
    pub fn add_file(&mut self, name: String, size: u32, key: u32) -> Result<(), Error> {
        match self.state {
            State::Header | State::FileHeader { index: 0, .. } => {}
            _ => {
                return Err(Error::InvalidState);
            }
        }

        let name_len = name.len();
        if name_len > MAX_FILE_NAME_LEN_USIZE {
            return Err(Error::FileNameTooLongUsize { len: name_len });
        }
        let name_len_u32 = u32::try_from(name_len).unwrap();

        self.files.push((name, size, key));
        self.data_position += (u32::try_from(U32_LEN).unwrap() * 4) + name_len_u32;

        Ok(())
    }

    /// Step the state machine, performing the action of writing the file headers.
    ///
    /// If the file headers have already been written, `Ok(WriterAction3::Done(()))` is returned and no work is performed.
    /// Calling this method is optional.
    /// The state machine will automatically write the header is if has not been written.
    pub fn step_write_file_headers(&mut self) -> Result<WriterAction3<()>, Error> {
        let (index, relative_data_offset) = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_write_header()?;
                    if !action.is_done() {
                        return Ok(action);
                    }
                }
                State::FileHeader {
                    index,
                    relative_data_offset,
                } => break (index, relative_data_offset),
                State::FileData { .. } => {
                    return Ok(WriterAction3::Done(()));
                }
            }
        };

        while *index < self.files.len() {
            let space = self.buffer.space();

            let (name, size, file_key) = &self.files[*index];
            let name_len = name.len();
            let name_len_u32 = u32::try_from(name_len).unwrap();
            let offset = self.data_position + *relative_data_offset;

            let expected_size = (4 * U32_LEN) + name_len;
            if space.len() < expected_size {
                return Ok(WriterAction3::Write);
            }

            let (offset_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
            let value = offset ^ self.key;
            offset_space.copy_from_slice(&value.to_le_bytes());

            let (size_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
            let value = size ^ self.key;
            size_space.copy_from_slice(&value.to_le_bytes());

            let (file_key_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
            let value = file_key ^ self.key;
            file_key_space.copy_from_slice(&value.to_le_bytes());

            let (name_len_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
            let value = name_len_u32 ^ self.key;
            name_len_space.copy_from_slice(&value.to_le_bytes());

            let name_space = &mut space[..name_len];
            name_space.copy_from_slice(name.as_bytes());
            crypt_name_bytes3(self.key, name_space);

            self.buffer.fill(expected_size);
            *index += 1;
            *relative_data_offset += size;
        }

        // Index is equal to the number of files here.
        // Write out terminator.
        let space = self.buffer.space();
        if space.len() < 4 * U32_LEN {
            return Ok(WriterAction3::Write);
        }

        let (offset_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
        let value = self.key;
        offset_space.copy_from_slice(&value.to_le_bytes());

        // I have no idea what these next 3 bytes mean.
        // They most certainly do exist,
        // as file data offsets would be off by 12 bytes if they didn't.
        // Their values seem to not matter.
        let (size_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
        let value = self.key;
        size_space.copy_from_slice(&value.to_le_bytes());
        let (offset_space, space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
        let value = self.key;
        offset_space.copy_from_slice(&value.to_le_bytes());
        let (offset_space, _space) = space.split_first_chunk_mut::<U32_LEN>().unwrap();
        let value = self.key;
        offset_space.copy_from_slice(&value.to_le_bytes());

        self.buffer.fill(4 * U32_LEN);

        let (size, key) = self
            .files
            .first()
            .map(|(_name, size, key)| (*size, *key))
            .unwrap_or((0, 0));
        self.state = State::FileData {
            index: 0,
            remaining: size,
            key,
            counter: 0,
        };

        Ok(WriterAction3::Done(()))
    }

    /// Step the state machine, performing the action of writing the file data.
    ///
    /// The state machine will automatically write the header is if has not been written.
    /// The state machine will automatically write the file headers if they have not been written.
    pub fn step_write_file_data(
        &mut self,
        file_index: usize,
        size: usize,
    ) -> Result<WriterAction3<usize>, Error> {
        let size_u32 = u32::try_from(size).expect("number of bytes written cannot fit in a `u32`");

        let (index, remaining, key, counter) = loop {
            match &mut self.state {
                State::Header => {
                    let action = self.step_write_header()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileHeader { .. } => {
                    let action = self.step_write_file_headers()?;
                    if !action.is_done() {
                        return Ok(action.map_done(|_| unreachable!()));
                    }
                }
                State::FileData {
                    index: expected_file_index,
                    remaining,
                    key,
                    counter,
                } => {
                    let (_file_name, file_size, _file_key) =
                        match self.files.get(*expected_file_index) {
                            Some(value) => value,
                            None => {
                                // We are done, the index has been advanced over all files.
                                return Ok(WriterAction3::Done(0));
                            }
                        };

                    if file_index != *expected_file_index {
                        // User advanced the file index, but we didn't.
                        // That means that the user thinks the file is done,
                        // but we don't, indicating truncated data.
                        if file_index == *expected_file_index + 1 {
                            return Err(Error::FileDataSizeMismatch {
                                actual: *file_size - *remaining,
                                expected: *file_size,
                            });
                        }

                        // We advanced the file index, but the user didn't.
                        // That means that we think the file is done,
                        // but the user doesn't, indicating extended data.
                        if file_index + 1 == *expected_file_index {
                            let (_, file_size, _) = self.files[file_index];

                            return Err(Error::FileDataSizeMismatch {
                                actual: file_size + size_u32,
                                expected: file_size,
                            });
                        }

                        // The user messed up badly.
                        // They advanced the file index much more than they should have.
                        // We can't guess what they mean, so just return an InvalidState error.
                        return Err(Error::InvalidState);
                    }

                    break (expected_file_index, remaining, key, counter);
                }
            }
        };

        let space = self.buffer.space();
        crypt_file_data(key, counter, &mut space[..size]);

        *remaining -= size_u32;
        if *remaining == 0 {
            let new_index = *index + 1;
            let (new_size, new_key) = self
                .files
                .get(new_index)
                .map(|(_name, size, key)| (*size, *key))
                .unwrap_or((0, 0));
            *index = new_index;
            *remaining = new_size;
            *key = new_key;
            *counter = 0;
        }
        self.buffer.fill(size);

        Ok(WriterAction3::Done(size))
    }
}

#[derive(Debug)]
enum State {
    Header,
    FileHeader {
        index: usize,
        relative_data_offset: u32,
    },
    FileData {
        index: usize,
        remaining: u32,
        key: u32,
        counter: u8,
    },
}
