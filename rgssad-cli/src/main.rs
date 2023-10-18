mod commands;

#[derive(Debug, argh::FromArgs)]
#[argh(description = "an extractor for rgssad archives")]
struct Options {
    #[argh(subcommand)]
    subcommand: Subcommand,
}

#[derive(Debug, argh::FromArgs)]
#[argh(subcommand)]
enum Subcommand {
    Unpack(self::commands::unpack::Options),
    Pack(self::commands::pack::Options),
}

fn main() -> anyhow::Result<()> {
    let options: Options = argh::from_env();

    match options.subcommand {
        Subcommand::Unpack(options) => {
            self::commands::unpack::exec(options)?;
        }
        Subcommand::Pack(options) => {
            self::commands::pack::exec(options)?;
        }
    }

    Ok(())
}
