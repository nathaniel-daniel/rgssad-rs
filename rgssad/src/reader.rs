mod buffer;

use self::buffer::Buffer;
use crate::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::VERSION;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

/// The state for when the Reader must read the header.
#[derive(Debug)]
enum State {
    // Header States
    ReadMagic {
        buffer: Buffer<[u8; 7]>,
    },
    ReadVersion,
    GetStreamPosition,

    // Entry States
    SeekEntry,
    ReadFileNameSize {
        buffer: Buffer<[u8; 4]>,
    },
    ReadFileName {
        buffer: Buffer<Vec<u8>>,
        encrypted: bool,
    },
    ReadFileSize {
        file_name: String,
        buffer: Buffer<[u8; 4]>,
    },
}

/// Read a u32 that has been encrypted.
fn read_decrypt_u32<R>(
    reader: R,
    buffer: &mut Buffer<[u8; 4]>,
    key: &mut u32,
) -> std::io::Result<u32>
where
    R: Read,
{
    // Read
    buffer.fill(reader)?;
    let mut n = u32::from_le_bytes(*buffer.buffer_ref());

    // Decrypt
    n ^= *key;
    *key = key.overflowing_mul(7).0.overflowing_add(3).0;

    Ok(n)
}

/// A reader for a "rgssad" archive file
#[derive(Debug)]
pub struct Reader<R> {
    /// The underlying reader.
    reader: R,

    /// The current encryption key.
    key: u32,

    /// The offset of the next entry, from the start of the reader.
    ///
    /// This is necessary as the inner reader object is passed to [`Entry`] objects,
    /// which may modify the position as they see fit.
    /// They are even allowed to not completely read all contents of the entry.
    next_entry_position: u64,

    state: State,
}

impl<R> Reader<R> {
    /// Create a new [`Reader`] with the default encryption key.
    pub fn new(reader: R) -> Reader<R> {
        Reader {
            reader,
            key: DEFAULT_KEY,
            next_entry_position: 0,
            state: State::ReadMagic {
                buffer: Buffer::new([0; 7]),
            },
        }
    }

    /// Get the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Get a reference to the reader.
    pub fn get_ref(&mut self) -> &R {
        &self.reader
    }

    /// Get a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R> Reader<R>
where
    R: Read + Seek,
{
    /// Read and validate the header.
    ///
    /// After this returns, call [`Reader::read_entry`] to read through entries.
    /// This function is a NOP if the header has already been read.
    pub fn read_header(&mut self) -> Result<(), Error> {
        loop {
            match &mut self.state {
                State::ReadMagic { buffer } => {
                    buffer.fill(&mut self.reader)?;
                    let magic = buffer.buffer_ref();
                    if magic != MAGIC {
                        return Err(Error::InvalidMagic { magic: *magic });
                    }
                    self.state = State::ReadVersion;
                    continue;
                }
                State::ReadVersion => {
                    // No need to persist, we can't have partial buffer fills since its a single byte.
                    let mut buffer = Buffer::new([0]);
                    buffer.fill(&mut self.reader)?;
                    let version = buffer.buffer_ref()[0];
                    if version != VERSION {
                        return Err(Error::InvalidVersion { version });
                    }

                    self.state = State::GetStreamPosition;
                }
                State::GetStreamPosition => {
                    self.next_entry_position = self.reader.stream_position()?;

                    // TODO: We can just jump to the read file name state since we haven't handed out an entry yet.
                    self.state = State::SeekEntry;

                    return Ok(());
                }
                _ => {
                    // We already read the header somehow.
                    return Ok(());
                }
            }
        }
    }

    /// Read the next entry from this archive.
    pub fn read_entry(&mut self) -> Result<Option<Entry<R>>, Error> {
        loop {
            match &mut self.state {
                State::SeekEntry => {
                    // Seek to start of entry.
                    //
                    // This is necessary as the user may have messed up our position by reading from the last entry.
                    self.reader
                        .seek(SeekFrom::Start(self.next_entry_position))?;

                    self.state = State::ReadFileNameSize {
                        buffer: Buffer::new([0; 4]),
                    };
                }
                State::ReadFileNameSize { buffer } => {
                    // We turn EOF errors into None here.
                    // This is because a missing file name (and by extension a missing file name size),
                    // indicate the end of the archive.
                    let size = match read_decrypt_u32(&mut self.reader, buffer, &mut self.key) {
                        Ok(size) => size,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::UnexpectedEof
                                && buffer.position() == 0 =>
                        {
                            return Ok(None);
                        }
                        Err(error) => {
                            return Err(Error::Io(error));
                        }
                    };

                    // Validate file name length.
                    //
                    // This is an extreme edge case,
                    // which can only occur if the following is true:
                    // 1. usize == u16
                    // 2. file_name.len() > u16::MAX
                    let size =
                        usize::try_from(size).map_err(|error| Error::FileNameTooLong { error })?;

                    self.state = State::ReadFileName {
                        buffer: Buffer::new(vec![0; size]),
                        encrypted: true,
                    };
                }
                State::ReadFileName { buffer, encrypted } => {
                    buffer.fill(&mut self.reader)?;

                    let file_name = buffer.buffer_mut();
                    if *encrypted {
                        for byte in file_name.iter_mut() {
                            // We mask with 0xFF, this cannot exceed the bounds of a u8.
                            *byte ^= u8::try_from(self.key & 0xFF).unwrap();
                            self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;
                        }
                        *encrypted = false;
                    }

                    // I'm fairly certain these are required to be ASCII, but I forget the source.
                    //
                    // TODO:
                    // Link source for ASCII file names, or do not assume ASCII file names.
                    let file_name = String::from_utf8(std::mem::take(file_name))
                        .map_err(|error| Error::InvalidFileName { error })?;

                    self.state = State::ReadFileSize {
                        file_name,
                        buffer: Buffer::new([0; 4]),
                    };
                }
                State::ReadFileSize { file_name, buffer } => {
                    let file_size = read_decrypt_u32(&mut self.reader, buffer, &mut self.key)?;

                    // Calculate the offset of the next entry:
                    // size_of(file_name_size) + size_of(file_name) + size_of(file_data_size) + size_of(file_data)
                    //
                    // We parsed the file size from a u32 so we can unwrap.
                    self.next_entry_position +=
                        4 + u64::try_from(file_name.len()).unwrap() + 4 + u64::from(file_size);

                    let file_name = std::mem::take(file_name);

                    self.state = State::SeekEntry;

                    return Ok(Some(Entry {
                        file_name,
                        size: file_size,
                        key: self.key,
                        reader: self.reader.by_ref().take(file_size.into()),
                        counter: 0,
                    }));
                }
                _ => {
                    return Err(Error::InvalidState);
                }
            }
        }
    }
}

/// An entry in an rgssad file
#[allow(dead_code)]
#[derive(Debug)]
pub struct Entry<'a, R> {
    /// The file path.
    pub(crate) file_name: String,

    /// The file size.
    pub(crate) size: u32,

    /// The current encryption key.
    pub(crate) key: u32,

    /// The inner reader.
    pub(crate) reader: std::io::Take<&'a mut R>,

    /// The inner counter, used for rotating the encryption key.
    ///
    /// This is necessary as the encryption key is rotated for every 4 bytes,
    /// but the [`Read`] object that we wrap does not need to obey these boundaries.
    counter: u8,
}

impl<R> Entry<'_, R> {
    /// The file path
    pub fn file_name(&self) -> &str {
        self.file_name.as_str()
    }

    /// The file size
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<R> Read for Entry<'_, R>
where
    R: Read,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        // Read encrypted bytes into the provided buffer.
        let n = self.reader.read(buffer)?;

        // Decrypt the encrypted bytes in-place.
        decrypt_entry_bytes(&mut buffer[..n], &mut self.key, &mut self.counter);

        Ok(n)
    }
}

// Decrypt the encrypted entry bytes in-place.
pub(crate) fn decrypt_entry_bytes(buffer: &mut [u8], key: &mut u32, counter: &mut u8) {
    for byte in buffer.iter_mut() {
        *byte ^= key.to_le_bytes()[usize::from(*counter)];
        if *counter == 3 {
            *key = key.overflowing_mul(7).0.overflowing_add(3).0;
        }
        *counter = (*counter + 1) % 4;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::*;

    #[test]
    fn reader_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Ensure skipping works.
        let mut num_skipped_entries = 0;
        while let Some(_entry) = reader.read_entry().expect("failed to read entry") {
            num_skipped_entries += 1;
        }

        // Reset position and recreate reader.
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        // Read entire archive into Vec.
        let mut entries = Vec::new();
        while let Some(mut entry) = reader.read_entry().expect("failed to read entry") {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
        }

        assert!(entries.len() == num_skipped_entries);
    }

    #[test]
    fn reader_trailing_bytes() {
        let mut file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        file.push(1);
        let file = std::io::Cursor::new(file);
        let mut reader = Reader::new(file);
        reader.read_header().expect("failed to read header");

        while let Ok(Some(_entry)) = reader.read_entry() {}

        let error = reader.read_entry().expect_err("reader should have errored");
        assert!(
            matches!(error, Error::Io(error) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}
