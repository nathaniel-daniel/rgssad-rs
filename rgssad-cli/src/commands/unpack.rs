use anyhow::bail;
use anyhow::ensure;
use anyhow::Context;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Component as PathComponent;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "unpack", description = "unpack an rgssad archive")]
pub struct Options {
    #[argh(positional, description = "the file to unpack")]
    pub input: PathBuf,

    #[argh(
        option,
        description = "the output directory",
        short = 'o',
        long = "output",
        default = "PathBuf::from(\".\")"
    )]
    pub output: PathBuf,
}

pub fn exec(options: Options) -> anyhow::Result<()> {
    let file = File::open(options.input)?;
    let mut reader = rgssad::Reader::new(file);
    reader.read_header()?;

    std::fs::create_dir_all(&options.output).with_context(|| {
        format!(
            "failed to create directory at \"{}\"",
            options.output.display()
        )
    })?;

    // This creates a UNC path on Windows, which is helpful when dealing with long paths.
    // This exists, as we just created it.
    let output = std::fs::canonicalize(&options.output)?;

    let mut last_error = Ok(());
    while let Some(mut file) = reader.read_file()? {
        println!("Extracting \"{}\"", file.name());

        // Sanitize and build path
        let out_path = match construct_out_path(&output, file.name().as_ref()) {
            Ok(out_path) => out_path,
            Err(error) => {
                eprintln!("  {error}, skipping");
                last_error =
                    Err(error).with_context(|| format!("failed to sanitize \"{}\"", file.name()));
                continue;
            }
        };

        match extract_file(&mut file, &out_path) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("  {error}, skipping");
                last_error = Err(error).with_context(|| {
                    format!(
                        "failed to extract \"{}\" to \"{}\"",
                        file.name(),
                        out_path.display()
                    )
                });
                continue;
            }
        }
    }

    last_error
}

fn construct_out_path(out_dir: &Path, file_path: &Path) -> anyhow::Result<PathBuf> {
    let mut out_path = out_dir.to_path_buf();
    let mut depth = 0_u32;

    for component in file_path.components() {
        match component {
            PathComponent::Prefix(_) => {
                bail!("encountered prefix in path");
            }
            PathComponent::RootDir => {
                bail!("encountered root dir in path");
            }
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                depth = depth.checked_sub(1).context("path goes above out path")?;
            }
            PathComponent::Normal(path) => {
                depth = depth.checked_add(1).context("path depth overflow")?;

                let path_has_prefix = Path::new(path)
                    .components()
                    .any(|path| matches!(path, PathComponent::Prefix(_)));
                let path_has_root = Path::new(path)
                    .components()
                    .any(|path| matches!(path, PathComponent::RootDir));

                ensure!(!path_has_prefix, "path component has prefix");
                ensure!(!path_has_root, "path component has root");

                out_path.push(path);
            }
        }
    }

    Ok(out_path)
}

fn extract_file(entry: &mut impl Read, out_path: &Path) -> anyhow::Result<()> {
    if let Some(parent_dir) = out_path.parent() {
        std::fs::create_dir_all(parent_dir)
            .with_context(|| format!("failed to create dir at \"{}\"", parent_dir.display()))?;
    }

    let mut file = File::options()
        .create_new(true)
        .write(true)
        .open(out_path)?;

    std::io::copy(entry, &mut file)?;

    file.flush()?;
    file.sync_all()?;

    Ok(())
}
