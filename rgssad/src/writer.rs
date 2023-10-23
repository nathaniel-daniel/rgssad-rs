use crate::Error;
use crate::DEFAULT_KEY;
use crate::MAGIC;
use crate::VERSION;
use std::io::Read;
use std::io::Write;

#[derive(Debug)]
pub struct Buffer<B> {
    buffer: B,
    position: usize,
}

impl<B> Buffer<B> {
    /// Make a new buffer.
    pub fn new(buffer: B) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    /// Make a new buffer from parts
    pub fn from_parts(buffer: B, position: usize) -> Self {
        Self { buffer, position }
    }
}

impl<B> Buffer<B>
where
    B: AsRef<[u8]>,
{
    /// Write from this buffer to a writer.
    ///
    /// This remembers how many bytes were written so that it may be retried on error without data loss.
    pub fn write<W>(&mut self, writer: W) -> std::io::Result<()>
    where
        W: Write,
    {
        write_all(writer, self.buffer.as_ref(), &mut self.position)
    }
}

fn write_all<W>(mut writer: W, buffer: &[u8], position: &mut usize) -> std::io::Result<()>
where
    W: Write,
{
    loop {
        if *position == buffer.len() {
            return Ok(());
        }

        match writer.write(&buffer[*position..]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(n) => {
                *position += n;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[derive(Debug)]
pub enum State {
    // Header States
    WriteMagic {
        buffer: Buffer<&'static [u8]>,
    },
    WriteVersion,

    // Entry States
    WriteEntryStart,
    WriteEntry {
        file_name_size_buffer: Buffer<[u8; 4]>,
        file_name_position: usize,
        file_size_buffer: Buffer<[u8; 4]>,
    },
    ReadEntryData {
        counter: u8,
        key: u32,
        bytes_written: u32,
    },
    WriteEntryData {
        counter: u8,
        key: u32,
        bytes_written: u32,
        position: usize,
        buffer_size: usize,
    },
}

/// 8Kb of space,
/// The same default that [`std::io::BufWriter`] uses.
const BUFFER_SIZE: usize = 8 * 1024;

/// The archive writer.
pub struct Writer<W, S> {
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

    /// The current state
    state: S,
}

impl<W, S> Writer<W, S> {
    /// Get the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> Writer<W, State> {
    /// Create an archive writer around a writer.
    pub fn new(writer: W) -> Writer<W, State> {
        Writer {
            writer,
            key: DEFAULT_KEY,
            buffer: vec![0; BUFFER_SIZE],
            state: State::WriteMagic {
                buffer: Buffer::new(MAGIC),
            },
        }
    }
}

impl<W> Writer<W, State>
where
    W: Write,
{
    /// Write the archive header.
    ///
    /// If the header has already been written, this is a NOP.
    pub fn write_header(&mut self) -> Result<(), Error> {
        loop {
            match &mut self.state {
                State::WriteMagic { ref mut buffer } => {
                    buffer.write(&mut self.writer)?;
                    self.state = State::WriteVersion;
                }
                State::WriteVersion => {
                    // We don't need a buffer here since this is 1 byte.
                    self.writer.write_all(std::slice::from_ref(&VERSION))?;
                    self.state = State::WriteEntryStart;
                    return Ok(());
                }
                _ => {
                    return Ok(());
                }
            }
        }
    }

    /// Finish writing.
    ///
    /// This is only a convenience function to call the inner [`Write`] object's [`Write::flush`] method.
    pub fn finish(&mut self) -> Result<(), Error> {
        match &mut self.state {
            State::WriteEntryStart => {}
            _ => {
                return Err(Error::InvalidState);
            }
        }

        self.writer.flush()?;
        Ok(())
    }
}

impl<W> Writer<W, State>
where
    W: Write,
{
    /// Write an entry.
    ///
    /// An entry is composed of a name, size, and data.
    /// This function may be retried.
    /// To retry, call this function with the same arguments.
    /// Note that if anything other than an I/O error occurs, the written bytes are likely corrupted.
    pub fn write_entry<R>(
        &mut self,
        file_name: &str,
        file_size: u32,
        mut file_data: R,
    ) -> Result<(), Error>
    where
        R: Read,
    {
        loop {
            match &mut self.state {
                State::WriteEntryStart => {
                    let mut file_name_len = u32::try_from(file_name.len())
                        .map_err(|error| Error::FileNameTooLong { error })?;

                    file_name_len ^= self.key;
                    self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;

                    self.buffer.clear();
                    self.buffer.extend(file_name.as_bytes());
                    for byte in self.buffer.iter_mut() {
                        *byte ^= u8::try_from(self.key & 0xFF).unwrap();
                        self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;
                    }

                    let file_size = file_size ^ self.key;
                    self.key = self.key.overflowing_mul(7).0.overflowing_add(3).0;

                    self.state = State::WriteEntry {
                        file_name_size_buffer: Buffer::new(file_name_len.to_le_bytes()),
                        file_name_position: 0,
                        file_size_buffer: Buffer::new(file_size.to_le_bytes()),
                    };
                }
                State::WriteEntry {
                    file_name_size_buffer,
                    file_name_position,
                    file_size_buffer,
                } => {
                    file_name_size_buffer.write(&mut self.writer)?;
                    write_all(&mut self.writer, &self.buffer, file_name_position)?;
                    file_size_buffer.write(&mut self.writer)?;

                    // Resize the scratch space to the requested buffer size.
                    self.buffer.clear();
                    self.buffer.resize(BUFFER_SIZE, 0);

                    self.state = State::ReadEntryData {
                        counter: 0,
                        key: self.key,
                        bytes_written: 0,
                    };
                }
                State::ReadEntryData {
                    counter,
                    key,
                    bytes_written,
                } => {
                    let n = file_data.read(&mut self.buffer)?;
                    if n == 0 {
                        if file_size != *bytes_written {
                            return Err(Error::FileDataSizeMismatch {
                                actual: *bytes_written,
                                expected: file_size,
                            });
                        }

                        self.state = State::WriteEntryStart;
                        return Ok(());
                    }

                    // We assume that the scratch buffer is smaller than 4 gigabytes.
                    *bytes_written = bytes_written
                        .checked_add(u32::try_from(n).expect("too many bytes written"))
                        .ok_or(Error::FileDataTooLong)?;

                    // TODO: We can possibly be more efficient here.
                    // If we are able to cast this to a slice of u32s,
                    // we can encrypt that instead and use this byte-wise impl only at the end.
                    for byte in self.buffer[..n].iter_mut() {
                        *byte ^= key.to_le_bytes()[usize::from(*counter)];
                        if *counter == 3 {
                            *key = key.overflowing_mul(7).0.overflowing_add(3).0;
                        }
                        *counter = (*counter + 1) % 4;
                    }

                    self.state = State::WriteEntryData {
                        counter: *counter,
                        key: *key,
                        bytes_written: *bytes_written,

                        position: 0,
                        buffer_size: n,
                    };
                }
                State::WriteEntryData {
                    counter,
                    key,
                    bytes_written,
                    position,
                    buffer_size,
                } => {
                    self.writer
                        .write_all(&self.buffer[*position..*buffer_size])?;

                    self.state = State::ReadEntryData {
                        counter: *counter,
                        key: *key,
                        bytes_written: *bytes_written,
                    };
                }
                _ => {
                    return Err(Error::InvalidState);
                }
            }
        }
    }
}
