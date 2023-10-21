use std::io::Read;

const ARCHIVE_PATH: &str = "Game.rgssad";

fn main() {
    // In a real app, you don't need to buffer.
    // You just need any object that implements Read + Seek.
    let file = std::fs::read(ARCHIVE_PATH).expect("failed to open archive");
    let file = std::io::Cursor::new(file);
    let mut reader = rgssad::Reader::new(file)
        .read_header()
        .expect("failed to read header");

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
