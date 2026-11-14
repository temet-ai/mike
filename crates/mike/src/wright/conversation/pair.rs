use anyhow::Context as _;
use clap::{Args, Subcommand};

use super::notice::PAGE_URL;
use super::token;

#[derive(Debug, Clone, Args)]
pub struct WrightArgs {
    #[command(subcommand)]
    pub command: WrightCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum WrightCommand {
    Pair(PairArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PairArgs {
    #[arg(long)]
    pub open: bool,
}

pub(crate) fn pair_url(token: &str) -> String {
    format!("{PAGE_URL}#pair={token}")
}

fn open_in_browser(url: &str) -> anyhow::Result<()> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(url)
        .spawn()
        .with_context(|| format!("could not start {program} to open the Wright page"))?;
    Ok(())
}

pub fn run(args: WrightArgs) -> anyhow::Result<()> {
    match args.command {
        WrightCommand::Pair(pair) => {
            let path = token::default_path().map_err(anyhow::Error::msg)?;
            let token = token::ensure(&path)
                .with_context(|| format!("cannot read or create {}", path.display()))?;
            let url = pair_url(token.expose());
            println!("{url}");
            if pair.open {
                open_in_browser(&url)?;
            }
            Ok(())
        }
    }
}
