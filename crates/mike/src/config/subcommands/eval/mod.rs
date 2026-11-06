mod alarms;
mod args_metrics;
mod args_runs;
mod fleet_kind;
mod gate;
mod import;
mod import_holdout;
mod rejudge;
mod rollup;
mod rollup_improvement;
mod run;
mod session_end;
mod session_end_defaults;
mod show;
mod withdraw_batch;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::CliOverrides;

pub use args_metrics::{
    EvalAlarmsBacktestArgs, EvalAlarmsCommand, EvalAlarmsListArgs, EvalRatesArgs, EvalRollupArgs,
};
pub use args_runs::{
    EvalGateArgs, EvalRejudgeArgs, EvalRunArgs, EvalSessionEndDefaultsArgs, EvalShowArgs,
};

#[derive(Debug, Clone, Subcommand)]
pub enum EvalCommand {
    #[command(subcommand)]
    Import(EvalImportSource),
    Rollup(EvalRollupArgs),
    Rates(EvalRatesArgs),
    #[command(subcommand)]
    Alarms(EvalAlarmsCommand),
    Run(Box<EvalRunArgs>),
    Gate(EvalGateArgs),
    Show(EvalShowArgs),
    WithdrawBatch(WithdrawBatchArgs),
    Rejudge(EvalRejudgeArgs),
    SessionEndDefaults(EvalSessionEndDefaultsArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum EvalImportSource {
    FleetMirror(FleetMirrorImportArgs),
    Holdout(HoldoutImportArgs),
}

#[derive(Debug, Clone, Args)]
pub struct HoldoutImportArgs {
    #[arg(long)]
    pub dir: PathBuf,
    #[arg(long)]
    pub batch: String,
    #[arg(long)]
    pub execute: bool,
    #[arg(long = "drop-split-conflicts", conflicts_with = "resplit_unused_dev")]
    pub drop_split_conflicts: bool,
    #[arg(long = "resplit-unused-dev")]
    pub resplit_unused_dev: bool,
}

#[derive(Debug, Clone, Args)]
pub struct WithdrawBatchArgs {
    pub batch: String,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FleetMirrorImportArgs {
    #[arg(long)]
    pub dir: PathBuf,
    #[arg(long)]
    pub execute: bool,
    #[arg(long = "control-turn", default_values_t = [16368])]
    pub control_turns: Vec<i64>,
}

pub(super) async fn run(command: EvalCommand, cli: CliOverrides) -> Result<()> {
    match command {
        EvalCommand::Import(source) => import_command(source, cli).await,
        EvalCommand::WithdrawBatch(args) => withdraw_batch::withdraw_batch(args, cli).await,
        EvalCommand::Rollup(args) => rollup::rollup(args, cli).await,
        EvalCommand::Rates(args) => rollup::rates(args, cli).await,
        EvalCommand::Alarms(command) => alarms_command(command, cli).await,
        EvalCommand::Run(args) => exit_with(run_command(*args, cli).await?),
        EvalCommand::Gate(args) => exit_with(gate_command(args, cli).await?),
        EvalCommand::Show(args) => exit_with(show::show(args, cli).await?),
        EvalCommand::Rejudge(args) => rejudge::rejudge(args, cli).await,
        EvalCommand::SessionEndDefaults(args) => {
            session_end_defaults::session_end_defaults(args, cli).await
        }
    }
}

async fn import_command(source: EvalImportSource, cli: CliOverrides) -> Result<()> {
    match source {
        EvalImportSource::FleetMirror(args) => import::fleet_mirror(args, cli).await,
        EvalImportSource::Holdout(args) => import_holdout::holdout(args, cli).await,
    }
}

async fn alarms_command(command: EvalAlarmsCommand, cli: CliOverrides) -> Result<()> {
    match command {
        EvalAlarmsCommand::Backtest(args) => alarms::backtest(args, cli).await,
        EvalAlarmsCommand::List(args) => alarms::list(args, cli).await,
    }
}

async fn run_command(args: EvalRunArgs, cli: CliOverrides) -> Result<i32> {
    if args.kind.as_deref() == Some("fleet") {
        fleet_kind::run(args, cli).await
    } else if args.role == "session_judge" {
        session_end::run(args, cli).await
    } else {
        run::run(args, cli).await
    }
}

async fn gate_command(args: EvalGateArgs, cli: CliOverrides) -> Result<i32> {
    if args.kind.as_deref() == Some("fleet") {
        fleet_kind::gate(&args)
    } else if args.role == "session_judge" {
        session_end::gate(args, cli).await
    } else {
        gate::gate(args, cli).await
    }
}

fn exit_with(code: i32) -> Result<()> {
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

pub(super) async fn connect(
    cli: CliOverrides,
    write: bool,
) -> Result<crate::store::persistence::PersistHandle> {
    use anyhow::{Context, bail};
    let config =
        crate::config::load::load(cli).context("failed to resolve config for mike eval")?;
    let Some(url) = config.database_url.as_deref() else {
        bail!("mike eval requires a database_url (set it in /settings or MIKE_DATABASE_URL)");
    };
    let handle = if write {
        crate::store::persistence::PersistHandle::connect_existing(url).await
    } else {
        crate::store::persistence::PersistHandle::connect_read_only(url).await
    };
    handle.context("mike eval: could not connect to the database")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::EvalCommand;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: EvalCommand,
    }

    #[test]
    fn rejudge_defaults_to_dry_run() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["x", "rejudge", "--since", "2026-07-27T00:00:00Z"])?;
        let EvalCommand::Rejudge(args) = cli.command else {
            anyhow::bail!("expected the rejudge subcommand");
        };
        assert!(!args.execute, "the default must be a dry run");
        assert!(!args.cancel && !args.status);
        assert_eq!(args.since.as_deref(), Some("2026-07-27T00:00:00Z"));
        assert_eq!(args.daily_cap, None);
        assert_eq!(
            args.role, "turn_judge",
            "eval_rejudge_role_defaults_to_turn_judge"
        );
        Ok(())
    }
}
