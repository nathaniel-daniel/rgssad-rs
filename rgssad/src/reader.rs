use crate::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::VERSION;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::marker::PhantomData;

/// The state for when the Reader must read the header.
pub struct HeaderState;

/// The state for when the Reader can read entries.
pub struct EntryState;

/// A reader for a "rgssad" archive file
#[derive(Debug)]
pub struct Reader<R, S> {
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

    state: PhantomData<S>,
}

impl<R, S> Reader<R, S> {
    /// Get the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> Reader<R, HeaderState>
where
    R: Read + Seek,
{
    /// Create a new [`Reader`] with the default encryption key.
    pub fn new(reader: R) -> Reader<R, HeaderState> {
        Reader {
            reader,
            key: DEFAULT_KEY,
            next_entry_position: 0,
            state: PhantomData,
        }
    }

    /// Read and validate the header.
    ///
    /// # Returns a new [`Reader`] that is ready to read entries.
    pub fn read_header(mut self) -> Result<Reader<R, EntryState>, Error> {
        let mut magic = [0; 7];
        self.reader.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic { magic });
        }

        let mut version = 0;
        self.reader.read_exact(std::slice::from_mut(&mut version))?;
        if version != VERSION {
            return Err(Error::InvalidVersion { version });
        }

        self.next_entry_position = self.reader.stream_position()?;
        Ok(Reader {
            reader: self.reader,
            key: self.key,
            next_entry_position: self.next_entry_position,
            state: PhantomData,
        })
    }
}

impl<R> Reader<R, EntryState>
where
    R: Read + Seek,
{
    /// Read a u32 that has been encrypted.
    fn read_decrypt_u32(&mut self) -> std::io::Result<u32> {
        let mut buffer = [0; 4];
        self.reader.read_exact(&mut buffer)?;
        let mut n = u32::from_le_bytes(buffer);
        n ^= self.key;
        self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;

        Ok(n)
    }

    /// Read the file name of the following entry.
    ///
    /// # Returns
    /// Returns an error if an I/O error occured.
    /// Returns `None` if at the end of the file.
    /// Returns the file name if successful.
    fn read_decrypt_file_name(&mut self) -> Result<Option<String>, Error> {
        // We turn EOF errors into None here.
        // This is because a missing file name (and by extension a missing file name size),
        // indicate the end of the archive.
        //
        // TODO:
        // The aforementioned approach is flawed in some edge cases;
        // trailing bytes that are less than 4 bytes long should become errors, not None.
        let size = match self.read_decrypt_u32() {
            Ok(file_name) => file_name,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        // Validate file name length.
        //
        // This is an extreme edge case,
        // which can only occur if the following is true:
        // 1. usize == u16
        // 2. file_name.len() > u16::MAX
        let size = usize::try_from(size).map_err(|error| Error::FileNameTooLong { error })?;

        let mut file_name = vec![0; size];
        self.reader.read_exact(&mut file_name)?;
        for byte in file_name.iter_mut() {
            // We mask with 0xFF, this cannot exceed the bounds of a u8.
            *byte ^= u8::try_from(self.key & 0xFF).unwrap();
            self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;
        }

        // I'm fairly certain these are required to be ASCII, but I forget the source.
        //
        // TODO:
        // Link source for ASCII file names, or do not assume ASCII file names.
        let file_name =
            String::from_utf8(file_name).map_err(|error| Error::InvalidFileName { error })?;

        Ok(Some(file_name))
    }

    /// Read the next entry from this archive.
    pub fn read_entry(&mut self) -> Result<Option<Entry<R>>, Error> {
        // Seek to start of entry.
        //
        // This is necessary as the user may have messed up our position by reading from the last entry.
        self.reader
            .seek(SeekFrom::Start(self.next_entry_position))?;

        // Read file name
        let file_name = match self.read_decrypt_file_name()? {
            Some(file_name) => file_name,
            None => {
                return Ok(None);
            }
        };

        // Read file size.
        let size = self.read_decrypt_u32()?;

        // Calculate the offset of the next entry.
        self.next_entry_position = self.reader.stream_position()? + u64::from(size);

        Ok(Some(Entry {
            file_name,
            size,
            key: self.key,
            reader: self.reader.by_ref().take(size.into()),
            counter: 0,
        }))
    }
}

/// An entry in an rgssad file
#[allow(dead_code)]
#[derive(Debug)]
pub struct Entry<'a, R> {
    /// The file path.
    file_name: String,

    /// The file size.
    size: u32,

    /// The current encryption key.
    key: u32,

    /// The inner reader.
    reader: std::io::Take<&'a mut R>,

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
        for byte in buffer[..n].iter_mut() {
            *byte ^= self.key.to_le_bytes()[usize::from(self.counter)];
            if self.counter == 3 {
                self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;
            }
            self.counter = (self.counter + 1) % 4;
        }

        Ok(n)
    }
}
