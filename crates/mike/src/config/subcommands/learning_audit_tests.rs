use chrono::{TimeZone, Utc};
use clap::Parser;

use super::super::ConfigCommand;
use super::{LearningAuditArgs, audit_end, default_output, weekly_boundary};

#[derive(Debug, Parser)]
struct TestCli {
    #[command(subcommand)]
    command: ConfigCommand,
}

fn instant(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
) -> anyhow::Result<chrono::DateTime<Utc>> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid test instant"))
}

fn args(weekly: bool, end: Option<&str>) -> LearningAuditArgs {
    LearningAuditArgs {
        weekly,
        end: end.map(str::to_owned),
        output: None,
        sync: false,
    }
}

#[test]
fn weekly_boundary_is_the_latest_monday_midnight_utc() -> anyhow::Result<()> {
    let wednesday = instant(2026, 11, 11, 15, 42)?;
    assert_eq!(weekly_boundary(wednesday), instant(2026, 11, 9, 0, 0)?);
    let monday_morning = instant(2026, 11, 9, 3, 5)?;
    assert_eq!(weekly_boundary(monday_morning), instant(2026, 11, 9, 0, 0)?);
    let sunday_night = instant(2026, 11, 15, 23, 59)?;
    assert_eq!(weekly_boundary(sunday_night), instant(2026, 11, 9, 0, 0)?);
    Ok(())
}

#[test]
fn audit_end_takes_weekly_then_end_then_now() -> anyhow::Result<()> {
    let now = instant(2026, 11, 11, 15, 42)?;
    assert_eq!(
        audit_end(&args(true, None), now)?,
        instant(2026, 11, 9, 0, 0)?
    );
    assert_eq!(
        audit_end(&args(false, Some("2026-11-02T00:00:00+00:00")), now)?,
        instant(2026, 11, 2, 0, 0)?
    );
    assert_eq!(
        audit_end(&args(false, Some("2026-11-02T02:00:00+02:00")), now)?,
        instant(2026, 11, 2, 0, 0)?
    );
    assert_eq!(audit_end(&args(false, None), now)?, now);
    Ok(())
}

#[test]
fn audit_end_without_a_timezone_names_the_value() -> anyhow::Result<()> {
    let now = instant(2026, 11, 11, 15, 42)?;
    let Err(error) = audit_end(&args(false, Some("2026-11-02T00:00:00")), now) else {
        anyhow::bail!("a timestamp without a timezone must be refused");
    };
    assert!(format!("{error:#}").contains("2026-11-02T00:00:00"));
    Ok(())
}

#[test]
fn weekly_and_end_together_are_refused() {
    let both = TestCli::try_parse_from([
        "mike",
        "learning-audit",
        "--weekly",
        "--end",
        "2026-11-09T00:00:00Z",
    ]);
    assert!(both.is_err());
}

#[test]
fn learning_audit_parses_the_scheduled_job_arguments() -> anyhow::Result<()> {
    let cli = TestCli::try_parse_from(["mike", "learning-audit", "--weekly", "--sync"])?;
    let ConfigCommand::LearningAudit(parsed) = cli.command else {
        anyhow::bail!("expected the LearningAudit variant");
    };
    assert!(parsed.weekly && parsed.sync);
    assert!(parsed.end.is_none() && parsed.output.is_none());
    Ok(())
}

#[test]
fn default_output_follows_the_checkout_and_falls_back_to_the_state_folder() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let home = directory.path().join("home");
    let checkout = directory.path().join("work/mike");
    std::fs::create_dir_all(checkout.join("scripts"))?;
    std::fs::write(checkout.join("scripts/learning-audit"), "")?;
    let installed = directory.path().join("prefix/share/mike");
    std::fs::create_dir_all(installed.join("scripts"))?;
    std::fs::write(installed.join("scripts/learning-audit"), "")?;
    let state = home.join(".local/state/mike/learning/artifacts/learning-audit");
    assert_eq!(
        default_output(
            &[directory.path().join("elsewhere"), checkout.clone()],
            &home
        ),
        checkout.join("artifacts/learning-audit")
    );
    assert_eq!(default_output(&[installed], &home), state);
    assert_eq!(default_output(&[], &home), state);
    Ok(())
}
