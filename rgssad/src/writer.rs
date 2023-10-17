use crate::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::VERSION;
use std::io::Read;
use std::io::Write;

/// 8Kb of space,
/// The same default that [`std::io::BufWriter`] uses.
const BUFFER_SIZE: usize = 8 * 1024;

/// The archive writer.
pub struct Writer<W> {
    /// The inner writer.
    writer: W,

    /// The current encryption key
    key: u32,

    /// A scratch space for encryption.
    ///
    /// Data must be encrypted before being written out.
    /// This encryption is performed in this scratch space.
    /// This cannot be done in-place, as we allow users to supply [`Read`] objects for file data.
    /// Methods that use the scratch space should not call other methods that use the scratch space while it is in use.
    /// The buffer should be cleared by its user, before each use.
    buffer: Vec<u8>,
}

impl<W> Writer<W> {
    /// Get the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> Writer<W>
where
    W: Write,
{
    /// Create an archive writer around a writer.
    pub fn new(writer: W) -> Result<Self, Error> {
        let mut writer = Self {
            writer,
            key: DEFAULT_KEY,
            buffer: vec![0; BUFFER_SIZE],
        };
        writer.write_header()?;

        Ok(writer)
    }

    /// Write the archive header.
    fn write_header(&mut self) -> Result<(), Error> {
        self.writer.write_all(MAGIC)?;
        self.writer.write_all(std::slice::from_ref(&VERSION))?;

        Ok(())
    }

    /// Write an encrypted u32.
    fn write_encrypt_u32(&mut self, mut value: u32) -> Result<(), Error> {
        value ^= self.key;

        self.writer.write_all(&value.to_le_bytes())?;
        self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;

        Ok(())
    }

    /// Write an encrypted file name.
    ///
    /// This method will use an internal buffer for scratch space.
    fn write_encrypt_file_name(&mut self, name: &str) -> Result<(), Error> {
        // Validate file name length.
        let len = u32::try_from(name.len()).map_err(|error| Error::FileNameTooLong { error })?;

        // Write file name size.
        self.write_encrypt_u32(len)?;

        // Clear buffer and insert file name data.
        self.buffer.clear();
        self.buffer.extend(name.as_bytes());

        // Encrypt file name data.
        for byte in self.buffer.iter_mut() {
            *byte ^= u8::try_from(self.key & 0xFF).unwrap();
            self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;
        }

        // Write out encrypted file name.
        self.writer.write_all(&self.buffer)?;

        Ok(())
    }

    /// Write an entry.
    ///
    /// An entry is composed of a name, size, and data.
    ///
    /// # Errors
    /// Returns an error if the number of file data bytes written does not match the file size.
    pub fn write_entry<R>(
        &mut self,
        file_name: &str,
        file_size: u32,
        mut file_data: R,
    ) -> Result<(), Error>
    where
        R: Read,
    {
        // Write the file name.
        self.write_encrypt_file_name(file_name)?;

        // Write the file size.
        self.write_encrypt_u32(file_size)?;

        // Resize the scratch space to the requested buffer size.
        self.buffer.clear();
        self.buffer.resize(BUFFER_SIZE, 0);

        let mut counter: u8 = 0;
        let mut key = self.key;
        let mut bytes_written = 0_u32;
        loop {
            let n = file_data.read(&mut self.buffer)?;
            if n == 0 {
                break;
            }

            // TODO: Error if exceeded.
            bytes_written += n as u32;

            for byte in self.buffer[..n].iter_mut() {
                *byte ^= key.to_le_bytes()[usize::from(counter)];
                if counter == 3 {
                    key = key.overflowing_mul(7).0.overflowing_add(3).0;
                }
                counter = (counter + 1) % 4;
            }

            self.writer.write_all(&self.buffer[..n])?;
        }

        if file_size != bytes_written {
            todo!();
        }

        Ok(())
    }
}
