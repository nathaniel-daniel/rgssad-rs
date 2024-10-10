# rgssad-rs
A Rust library for reading and writing RPGMaker XP, VX, and VX ACE archives from Rust.
This currently includes complete support for "rgssad", "rgss2a" files and
partial support for "rgss3a" files.

## Limitations
Currently, the entire format of an "rgss3a" file is not known.
After the last file header with a 0 offset, there are 12 bytes of unknown purpose.
It is possible that it's just garbage data, but these bytes may have an unknown purpose like a checksum.
This means that files cannot be round tripped while remaining byte-for-byte compatible.
In practice, written files seem to work fine.

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
    let mut files = Vec::new();
    while let Some(mut file) = reader.read_file().expect("failed to read file") {
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).expect("failed to read file");
        files.push((file.name().to_string(), buffer));
    }

    // Write all files into new archive.
    let mut new_file = Vec::new();
    let mut writer = rgssad::Writer::new(&mut new_file);
    writer.write_header().expect("failed to write header");
    for (file_name, file_data) in files.iter() {
        writer
            .write_file(
                file_name,
                u32::try_from(file_data.len()).expect("file data too large"),
                &**file_data,
            )
            .expect("failed to write file");
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
See [FileFormat](FileFormat.md)

## Related Projects
The following sources may be helpful to confirm the correctness of this implementation.
 * https://github.com/Kriper1111/YARE-py
 * https://github.com/uuksu/RPGMakerDecrypter
 * https://github.com/KatyushaScarlet/RGSS-Extractor
 * https://aluigi.altervista.org/quickbms.htm
 * https://github.com/dogtopus/rgssad-fuse

## License
Licensed under either of
 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
