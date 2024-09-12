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
        let mut num_skipped_files = 0;
        while let Some(_file) = reader.read_file().await.expect("failed to read file") {
            num_skipped_files += 1;
        }

        // Reset position and recreate reader.
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        let mut files = Vec::new();
        while let Some(mut file) = reader.read_file().await.expect("failed to read file") {
            let mut buffer: Vec<u8> = Vec::new();
            file.read_to_end(&mut buffer)
                .await
                .expect("failed to read file");

            files.push((file.name().to_string(), buffer));
        }

        assert!(files.len() == num_skipped_files);

        // Validate with sync impl
        let mut file = reader.into_inner();
        file.seek(SeekFrom::Start(0))
            .expect("failed to seek to start");
        let mut reader = crate::Reader::new(file);
        reader.read_header().expect("failed to read header");

        let mut files_sync = Vec::new();
        while let Some(mut file) = reader.read_file().expect("failed to read file") {
            use std::io::Read;

            let mut buffer: Vec<u8> = Vec::new();
            file.read_to_end(&mut buffer).expect("failed to read file");
            files_sync.push((file.name().to_string(), buffer));
        }

        assert!(files == files_sync);
    }

    #[tokio::test]
    async fn reader_writer_smoke() {
        let file = std::fs::read(VX_TEST_GAME).expect("failed to open archive");
        let file = std::io::Cursor::new(file);
        let mut reader = TokioReader::new(file);
        reader.read_header().await.expect("failed to read header");

        // Read entire archive into Vec.
        let mut files = Vec::new();
        while let Some(mut file) = reader.read_file().await.expect("failed to read file") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .await
                .expect("failed to read file");
            files.push((file.name().to_string(), buffer));
        }

        // Write all files into new archive.
        let mut new_file = Vec::<u8>::new();
        let mut writer = TokioWriter::new(&mut new_file);
        writer.write_header().await.expect("failed to write header");
        for (file_name, file_data) in files.iter() {
            writer
                .write_entry(
                    file_name,
                    u32::try_from(file_data.len()).expect("file data too large"),
                    &**file_data,
                )
                .await
                .expect("failed to write file");
        }
        writer.finish().await.expect("failed to flush");

        let file = reader.into_inner();

        // Ensure archives are byte-for-byte equivalent.
        assert!(&new_file == file.get_ref());
    }
}
