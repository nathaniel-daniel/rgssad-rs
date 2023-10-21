use anyhow::Context;
use std::fs::File;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, argh::FromArgs)]
#[argh(
    subcommand,
    name = "pack",
    description = "pack a directory into an archive"
)]
pub struct Options {
    #[argh(positional, description = "the path to the folder to pack")]
    pub input: PathBuf,

    #[argh(positional, description = "the output file path")]
    pub output: PathBuf,
}

pub fn exec(options: Options) -> anyhow::Result<()> {
    let mut output_file = File::options()
        .create_new(true)
        .write(true)
        .open(&options.output)
        .with_context(|| format!("failed to open \"{}\"", options.output.display()))?;
    let mut writer = rgssad::Writer::new(&mut output_file).write_header()?;

    for entry in WalkDir::new(&options.input).sort_by_file_name() {
        let entry = entry?;
        let file_type = entry.file_type();
        let path = entry.path();

        if file_type.is_dir() {
            continue;
        }

        let relative_path = path
            .strip_prefix(&options.input)
            .with_context(|| format!("failed to make relative path from \"{}\"", path.display()))?;

        let relative_path_str = relative_path.to_str().with_context(|| {
            format!(
                "relative path \"{}\" contains invalid unicode",
                path.display()
            )
        })?;

        println!("Packing \"{relative_path_str}\"");

        let file =
            File::open(path).with_context(|| format!("failed to open \"{}\"", path.display()))?;
        let file_metadata = file
            .metadata()
            .with_context(|| format!("failed to get metadata for \"{}\"", path.display()))?;
        let file_size = u32::try_from(file_metadata.len())
            .with_context(|| format!("file \"{}\" is too large", path.display()))?;

        writer.write_entry(relative_path_str, file_size, file)?;
    }
    writer.finish()?;

    output_file.sync_all()?;

    Ok(())
}
