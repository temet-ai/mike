use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

use super::super::ConfigCommand;

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[test]
fn compressor_loop_defaults_to_seven_days_without_applying() -> Result<()> {
    let cli = TestCli::try_parse_from(["mike", "compressor-loop"])?;
    let ConfigCommand::CompressorLoop(args) = cli.command else {
        bail!("expected the CompressorLoop variant");
    };
    assert_eq!((args.days, args.apply, args.evidence), (7, false, None));
    Ok(())
}

#[test]
fn compressor_loop_parses_days_apply_and_evidence() -> Result<()> {
    let cli = TestCli::try_parse_from([
        "mike",
        "compressor-loop",
        "--days",
        "3",
        "--apply",
        "--evidence",
        "qualification.json",
    ])?;
    let ConfigCommand::CompressorLoop(args) = cli.command else {
        bail!("expected the CompressorLoop variant");
    };
    assert_eq!(args.days, 3);
    assert!(args.apply);
    assert_eq!(args.evidence, Some(PathBuf::from("qualification.json")));
    Ok(())
}

#[test]
fn compressor_loop_rejects_a_non_numeric_window() {
    assert!(TestCli::try_parse_from(["mike", "compressor-loop", "--days", "week"]).is_err());
}
