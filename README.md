# rgssad-rs
A Rust library for reading and writing RPGMaker XP and RPGMaker VX archives from Rust.
This currently includes support for "rgssad" and "rgss2a" files.


Note that there is currently no support for RPGMaker VX Ace "rgss3a" files.
Through superficially similar, the internal structure of the file format is very different from prior versions.
Allowing these files to be parsed with the same interface would greatly increase code complexity.
In the future, support for these files may be added via another `Rgss3aReader` type.

## Example
```rust
use std::io::Read;

const ARCHIVE_PATH: &str = "Game.rgssad";

fn main() {
    // In a real app, you don't need to buffer.
    // You just need any object that implements Read + Seek.
    let file = std::fs::read(ARCHIVE_PATH).expect("failed to open archive");
    let file = std::io::Cursor::new(file);
    let mut reader = rgssad::Reader::new(file);
    reader.read_header().expect("failed to read header");

    // Read entire archive into Vec.
    let mut entries = Vec::new();
    while let Some(mut entry) = reader.read_entry().expect("failed to read entry") {
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer).expect("failed to read file");
        entries.push((entry.file_name().to_string(), buffer));
    }

    // Write all entries into new archive.
    let mut new_file = Vec::new();
    let mut writer = rgssad::Writer::new(&mut new_file)
        .write_header()
        .expect("failed to write header");
    for (file_name, file_data) in entries.iter() {
        writer
            .write_entry(
                file_name,
                u32::try_from(file_data.len()).expect("file data too large"),
                &**file_data,
            )
            .expect("failed to write entry");
    }
    writer.finish().expect("failed to finish");

    // Get the inner archive byte vec, so that we can compare it with the new archive.
    let file = reader.into_inner();

    // The old archive and new archive are byte-for-byte equivalent.
    assert!(&new_file == file.get_ref());
}
```

## Features
| Name  | Description                                      |
|-------|--------------------------------------------------|
| tokio | Enable the tokio wrappers for use in async code. |

## Docs
Master: https://nathaniel-daniel.github.io/rgssad-rs/rgssad/

## CLI
This repository also contains a small CLI to unpack and repack these archives.

### Installing
This small CLI may be installed with the following:
```bash
cargo install --force --git https://github.com/nathaniel-daniel/rgssad-rs
```

### Usage
Unpacking may be done with the following:
```bash
rgssad-cli unpack path-to-archive.rgssad -o path-to-output-directory
```

Packing may be done with the following:
```bash
rgssad-cli pack path-to-directory path-to-new-archive.rgssad
```

## Testing
Currently, only `rgssad` has tests; the CLI is not tested.
Tests may be run with the following command:
```bash
cargo test
```
Use the following command to test the tokio wrappers:
```bash
cargo test --features=tokio
```

## Try it Online
You can test an online version of this library at https://nathaniel-daniel.github.io/rgssad-online-viewer/

## File Format
This is an attempted specification for the file format of "rgssad" Archives.

### Encryption
This file format uses a primitive obfuscation scheme.
A static encryption key `0xDEAD_BEEF`, is XORed with most of the contents of the file and rotated at specific intervals described below.

### Key Rotation
Key rotation is defined as: `new_key = (old_key * 7) + 3`.

#### Header
The header is unencrypted.

#### `file_name_size`
The `EntryHeader` type's `file_name_size` is encrypted, and requires a key rotation after being read.
See the `encrypted32` type for more info.

#### `file_name`
The `EntryHeader` type's `file_name` is encrypted and requires a key rotation after each byte.
See the `encrypted8` type for more info.
Note that this field is fairly special, as only the lowest byte of the key is used for the XOR.
Furthermore, the key rotation occurs each byte instead of every 4th byte, 
unlike the other key rotations that occur in this file.

#### `file_size`
The `EntryHeader` type's `file_size` is encrypted, and requires a key rotation after being read.
See the `encrypted32` type for more info.

#### `data`
The `FileData` type's `data` is encrypted and requires a key rotation every 4 bytes.
When decrypting, the data field should be casted to an encrypted32 array with 0 padding, performing the encryption, the casting the result into a byte array while trimming the padding.
See the `encrypted8_4` type for more info.
This field uses the key produced after the last `file_size` field.
The key used to decrypt this type should not be persisted;
the next `EntryHeader` should also use the key from the last `file_size` field.

### Types

#### Header
The header struct is as follows. 
The magic field MUST be `"RGSSAD\0"`.
The version field MUST be 1.
```c
struct Header {
    u8 magic[7];
    u8 version;
}
```

#### encrypted32
A little-endian u32 value that has been encrypted with the file's key.
Decryption is performed by a simple XOR with the file key: `unencryped = encrypted ^ key`.
After decrypting, the current key must be rotated.

#### encrypted8
A u8 value that has been encrypted with the file's key.
Decryption is performed by a simple XOR with the lowest byte of the file key: `unencryped = encrypted ^ (key & 0xFF)`.
After decrypting, the current key must be rotated.

#### encrypted8_4
A u8 value that has been encrypted with the file's key.
This type is only used as part of an array.
Decryption is performed by casting the array to an `encrypted32` array with 0 padding, 
performing the decryption as one would do with an array of `encrypted32` values,
then casting the result into a byte buffer and removing the padding.
Note that key rotations occur every 4 bytes and not every byte.
Note that the entire key is used for decrypting.

#### EntryHeader
The entry header.
If is decrypted with the value of the key after the last `file_size` field, 
or `0xDEAD_BEEF` if an `EntryHeader` has not been processed yet.
```c
struct Entry {
    encrypted32 file_name_size;
    encrypted8 file_name[file_name_size];
    encrypted32 file_size;
}
```

#### EntryData
The file data for an entry. 
It uses the encryption key produced after the `file_size` field.
The encryption key used for decrypting this type is not persisted;
the next `EntryHeader` is read with the encryption key after the last `file_size` field.
N is determined from the `file_size` field from the preceding `EntryHeader`.
```c
struct EntryData {
    encrypted8_4 data[N];
}
```

#### Entry
```c
struct Entry {
    EntryHeader header;
    EntryData data;
}
```

#### File
The overall file.
N is determined by reading entries until EOF is reached.
```
struct File {
    Header header;
    Entry entries[N];
}
```

## License
Licensed under either of
 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
