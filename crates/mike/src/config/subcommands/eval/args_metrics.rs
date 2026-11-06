use chrono::{DateTime, NaiveDate, Utc};
use clap::{Args, Subcommand};

#[derive(Debug, Clone, Args)]
pub struct EvalRollupArgs {
    #[arg(long, conflicts_with_all = ["from", "to"])]
    pub day: Option<NaiveDate>,
    #[arg(long, requires = "to")]
    pub from: Option<NaiveDate>,
    #[arg(long, requires = "from")]
    pub to: Option<NaiveDate>,
    #[arg(long)]
    pub execute: bool,
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EvalRatesArgs {
    #[arg(long, value_parser = ["accuracy", "label_mix", "base_rate", "steer_calibration"])]
    pub family: String,
    #[arg(long)]
    pub role: Option<String>,
    #[arg(long = "label-source")]
    pub label_source: Option<String>,
    #[arg(long)]
    pub class: Option<String>,
    #[arg(long)]
    pub from: Option<DateTime<Utc>>,
    #[arg(long)]
    pub to: Option<DateTime<Utc>>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EvalAlarmsCommand {
    Backtest(EvalAlarmsBacktestArgs),
    List(EvalAlarmsListArgs),
}

#[derive(Debug, Clone, Args)]
pub struct EvalAlarmsBacktestArgs {
    #[arg(long, value_parser = [
        "label_mix_shift", "unjudged_backlog", "judge_stalled", "budget_refusals",
        "change_without_gate", "gold_label_by_agent",
    ])]
    pub kind: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long, default_value = "1d", value_parser = ["1d", "1h"])]
    pub step: String,
}

#[derive(Debug, Clone, Args)]
pub struct EvalAlarmsListArgs {
    #[arg(long, default_value = "open", value_parser = ["open", "cleared", "all"])]
    pub state: String,
}

pub(super) fn parse_instant(raw: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        return Ok(at.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map(crate::eval_metrics::day_start)
        .map_err(|e| format!("{raw:?} is neither RFC 3339 nor YYYY-MM-DD: {e}"))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{EvalAlarmsCommand, EvalRatesArgs, EvalRollupArgs, parse_instant};

    #[derive(Parser)]
    struct Rollup {
        #[command(flatten)]
        args: EvalRollupArgs,
    }

    #[derive(Parser)]
    struct Rates {
        #[command(flatten)]
        args: EvalRatesArgs,
    }

    #[derive(Parser)]
    struct Alarms {
        #[command(subcommand)]
        command: EvalAlarmsCommand,
    }

    #[test]
    fn eval_rollup_defaults_to_dry_run() -> anyhow::Result<()> {
        let parsed = Rollup::try_parse_from(["x", "--day", "2026-09-24"])?;
        assert!(!parsed.args.execute);
        Ok(())
    }

    #[test]
    fn eval_alarms_backtest_requires_kind_from_to() {
        assert!(Alarms::try_parse_from(["x", "backtest", "--kind", "judge_stalled"]).is_err());
        assert!(
            Alarms::try_parse_from([
                "x",
                "backtest",
                "--kind",
                "judge_stalled",
                "--from",
                "2026-07-30",
                "--to",
                "2026-08-03"
            ])
            .is_ok()
        );
    }

    #[test]
    fn eval_alarms_backtest_rejects_unknown_kind() {
        let parsed = Alarms::try_parse_from([
            "x",
            "backtest",
            "--kind",
            "metric_drop",
            "--from",
            "a",
            "--to",
            "b",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn eval_rates_parses_rfc3339_window() -> anyhow::Result<()> {
        let parsed = Rates::try_parse_from([
            "x",
            "--family",
            "accuracy",
            "--from",
            "2026-08-31T00:00:00Z",
            "--to",
            "2026-09-23T17:22:00Z",
        ])?;
        assert!(parsed.args.from.is_some() && parsed.args.to.is_some());
        Ok(())
    }

    #[test]
    fn a_plain_date_is_its_day_start() -> anyhow::Result<()> {
        let at = parse_instant("2026-09-01").map_err(anyhow::Error::msg)?;
        assert_eq!(at.to_rfc3339(), "2026-09-01T00:00:00+00:00");
        assert!(parse_instant("yesterday").is_err());
        Ok(())
    }
}
