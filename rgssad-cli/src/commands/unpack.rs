use anyhow::anyhow;
use anyhow::Context;
use std::fs::File;
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
    let mut reader = rgssad::Reader::new(file)?;

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
    'entry_loop: while let Some(entry) = reader.read_entry()? {
        println!("Extracting \"{}\"", entry.file_name());

        // Sanitize and build path
        let mut out_path = construct_out_path(&output, entry.file_name().as_ref()).unwrap();

        /*
        let mut depth = 0_i32;
        for component in Path::new(entry.file_name()).components() {
            match component {
                PathComponent::Prefix(_) => {
                    println!("  encountered prefix in path, skipping");
                    last_error = Err(anyhow!(
                        "failed to extract \"{}\", encountered prefix in path",
                        entry.file_name()
                    ));

                    break 'entry_loop;
                }
                PathComponent::RootDir => {
                    println!("  encountered root dir in path, skipping");
                    last_error = Err(anyhow!(
                        "failed to extract \"{}\", encountered root dir in path",
                        entry.file_name()
                    ));

                    break 'entry_loop;
                }
                PathComponent::CurDir => {}
                PathComponent::ParentDir => {
                    depth = match depth.checked_sub(1) {
                        Some(depth) => depth,
                        None => {
                            println!("  encountered root dir in path, skipping");
                        }
                    }
                }
                _ => {
                    dbg!(component);
                }
            }
        }
        */
    }

    last_error
}

fn construct_out_path(out_dir: &Path, file_path: &Path) -> anyhow::Result<PathBuf> {
    todo!()
}
