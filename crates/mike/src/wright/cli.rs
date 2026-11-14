use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Args)]
pub struct WrightArgs {
    #[command(subcommand)]
    pub command: WrightCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum WrightCommand {
    Pair(crate::wright::conversation::pair::PairArgs),
    #[command(hide = true)]
    SandboxRelay(SandboxRelayArgs),
}

#[derive(Debug, Clone, Args)]
pub struct SandboxRelayArgs {
    #[arg(long)]
    pub socket: PathBuf,
    #[arg(long, required = true)]
    pub listen: Vec<SocketAddr>,
    #[arg(last = true, required = true)]
    pub command: Vec<OsString>,
}

pub async fn run(args: WrightArgs) -> i32 {
    match args.command {
        WrightCommand::Pair(pair) => match crate::wright::conversation::pair::run(&pair) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("mike wright pair: {e:#}");
                1
            }
        },
        WrightCommand::SandboxRelay(relay) => {
            crate::wright::investigate::relay_inside::run(relay.socket, relay.listen, relay.command)
                .await
        }
    }
}
