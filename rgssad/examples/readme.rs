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
