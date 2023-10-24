mod adapters;
pub mod reader;
pub mod writer;

use self::adapters::AsyncRead2Read;
use self::adapters::AsyncWrite2Write;
pub use self::reader::TokioReader;
pub use self::writer::TokioWriter;

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::VX_TEST_GAME;
    use std::io::Seek;
    use std::io::SeekFrom;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn reader_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        // Ensure skipping works.
        let mut num_skipped_entries = 0;
        while let Some(_entry) = reader.read_entry().await.expect("failed to read entry") {
            num_skipped_entries += 1;
        }

        // Reset position and recreate reader.
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        let mut entries = Vec::new();
        while let Some(mut entry) = reader.read_entry().await.expect("failed to read entry") {
            let mut buffer: Vec<u8> = Vec::new();
            entry
                .read_to_end(&mut buffer)
                .await
                .expect("failed to read file");

            entries.push((entry.file_name().to_string(), buffer));
        }

        assert!(entries.len() == num_skipped_entries);

        // Validate with sync impl
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = crate::Reader::new(file);
        reader.read_header().expect("failed to read header");

        let mut entries_sync = Vec::new();
        while let Some(mut entry) = reader.read_entry().expect("failed to read entry") {
            use std::io::Read;

            let mut buffer: Vec<u8> = Vec::new();
            entry.read_to_end(&mut buffer).expect("failed to read file");
            entries_sync.push((entry.file_name().to_string(), buffer));
        }

        assert!(entries == entries_sync);
    }

    #[tokio::test]
    async fn reader_writer_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        // Read entire archive into Vec.
        let mut entries = Vec::new();
        while let Some(mut entry) = reader.read_entry().await.expect("failed to read entry") {
            let mut buffer = Vec::new();
            entry
                .read_to_end(&mut buffer)
                .await
                .expect("failed to read file");
            entries.push((entry.file_name().to_string(), buffer));
        }

        // Write all entries into new archive.
        let mut new_file = Vec::<u8>::new();
        let mut writer = TokioWriter::new(&mut new_file);
        writer.write_header().await.expect("failed to write header");
        for (file_name, file_data) in entries.iter() {
            writer
                .write_entry(
                    file_name,
                    u32::try_from(file_data.len()).expect("file data too large"),
                    &**file_data,
                )
                .await
                .expect("failed to write entry");
        }
        writer.finish().await.expect("failed to flush");

        let file = reader.into_inner();

        // Ensure archives are byte-for-byte equivalent.
        assert!(&new_file == file.get_ref());
    }
}
