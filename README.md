# rgssad-rs
A Rust library for reading and writing RPGMaker XP and RPGMaker VX archives from Rust.
This currently includes support for ".rgssad" and "rgssa2" files.

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

## Try it Online
You can test an online version of this library at https://nathaniel-daniel.github.io/rgssad-online-viewer/

## License
Licensed under either of
 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
