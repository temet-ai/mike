use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct EvalRunArgs {
    #[arg(long)]
    pub rubric: Option<String>,
    #[arg(long, default_value = "judge-input-v2.1")]
    pub builder: String,
    #[arg(long, value_delimiter = ',')]
    pub slices: Vec<String>,
    #[arg(long = "prompt-file")]
    pub prompt_file: Option<PathBuf>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long, default_value = "dev", value_parser = ["dev", "holdout"])]
    pub split: String,
    #[arg(long, value_delimiter = ',', default_values_t = ["seed".to_owned(), "gold".to_owned(), "relayed".to_owned()])]
    pub tiers: Vec<String>,
    #[arg(long, value_delimiter = ',', default_values_t = [
        "double_blind_adjudicated".to_owned(), "operator_click".to_owned(),
        "operator_confirmed_relay".to_owned(), "relayed_quote".to_owned(),
    ])]
    pub methods: Vec<String>,
    #[arg(long = "holdout-batch")]
    pub holdout_batch: Option<String>,
    #[arg(long = "labels-as-of", conflicts_with = "same_labels_as")]
    pub labels_as_of: Option<DateTime<Utc>>,
    #[arg(long = "same-labels-as")]
    pub same_labels_as: Option<i64>,
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long, default_value = "measure", value_parser = ["measure", "noise", "ablation"])]
    pub purpose: String,
    #[arg(long = "repeat-of")]
    pub repeat_of: Option<i64>,
    #[arg(long = "max-usd", default_value_t = 1.0)]
    pub max_usd: f64,
    #[arg(long)]
    pub resume: Option<i64>,
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long, value_parser = ["turn_judge", "session_judge"], default_value = "turn_judge")]
    pub role: String,
    #[arg(long, conflicts_with_all = ["rubric", "builder", "slices", "prompt_file", "model", "resume"])]
    pub bundle: Option<i64>,
    #[arg(long = "kind", value_parser = ["fleet"])]
    pub kind: Option<String>,
    #[arg(long = "record-file", requires = "kind")]
    pub record_file: Option<PathBuf>,
    #[arg(long = "fleet-run", requires = "kind")]
    pub fleet_run: Option<uuid::Uuid>,
}

impl EvalGateArgs {
    pub fn candidate_run(&self) -> anyhow::Result<i64> {
        self.candidate
            .ok_or_else(|| anyhow::anyhow!("mike eval gate needs --candidate <run id>"))
    }
}

#[derive(Debug, Clone, Args)]
pub struct EvalGateArgs {
    #[arg(long, alias = "candidate-run", required_unless_present = "kind_fleet")]
    pub candidate: Option<i64>,
    #[arg(long)]
    pub baseline: Option<i64>,
    #[arg(long = "kind", id = "kind_fleet", value_parser = ["fleet"])]
    pub kind: Option<String>,
    #[arg(long = "baseline-record", requires = "candidate_record")]
    pub baseline_record: Option<PathBuf>,
    #[arg(long = "candidate-record", id = "candidate_record")]
    pub candidate_record: Option<PathBuf>,
    #[arg(long = "history-record")]
    pub history_records: Vec<PathBuf>,
    #[arg(long, value_parser = ["turn_judge", "session_judge"], default_value = "turn_judge")]
    pub role: String,
    #[arg(long, alias = "noise-run")]
    pub noise: Option<i64>,
    #[arg(long, default_value = "promotion", value_parser = ["promotion", "e-slice"])]
    pub rule: String,
    #[arg(long = "override-reason")]
    pub override_reason: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EvalShowArgs {
    pub id: i64,
    #[arg(long)]
    pub gate: bool,
    #[arg(long)]
    pub items: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EvalRejudgeArgs {
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long = "daily-cap")]
    pub daily_cap: Option<i32>,
    #[arg(long)]
    pub execute: bool,
    #[arg(long)]
    pub status: bool,
    #[arg(long)]
    pub cancel: bool,
    #[arg(long, value_parser = ["turn_judge", "session_judge"], default_value = "turn_judge")]
    pub role: String,
}

#[derive(Debug, Clone, Args)]
pub struct EvalSessionEndDefaultsArgs {
    #[arg(long, default_value = "fleet_mirror/session_ends")]
    pub source: String,
    #[arg(long, value_delimiter = ',', default_values_t = ["seed".to_owned(), "gold".to_owned()])]
    pub tiers: Vec<String>,
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{EvalGateArgs, EvalRunArgs, EvalShowArgs};

    #[derive(Parser)]
    struct Run {
        #[command(flatten)]
        args: EvalRunArgs,
    }

    #[derive(Parser)]
    struct Gate {
        #[command(flatten)]
        args: EvalGateArgs,
    }

    #[derive(Parser)]
    struct Show {
        #[command(flatten)]
        args: EvalShowArgs,
    }

    #[test]
    fn eval_run_parses_with_defaults() -> anyhow::Result<()> {
        let parsed = Run::try_parse_from(["x"])?.args;
        assert_eq!(parsed.split, "dev");
        assert_eq!(parsed.builder, "judge-input-v2.1");
        assert!((parsed.max_usd - 1.0).abs() < f64::EPSILON);
        assert!(!parsed.dry_run);
        assert_eq!(parsed.tiers, ["seed", "gold", "relayed"]);
        Ok(())
    }

    #[test]
    fn eval_gate_requires_a_candidate_and_takes_an_optional_baseline() -> anyhow::Result<()> {
        assert!(Gate::try_parse_from(["x"]).is_err());
        assert!(Gate::try_parse_from(["x", "--candidate", "1", "--baseline", "2"]).is_ok());
        let session =
            Gate::try_parse_from(["x", "--role", "session_judge", "--candidate-run", "5"])?.args;
        assert_eq!(session.baseline, None);
        assert_eq!(session.role, "session_judge");
        Ok(())
    }

    #[test]
    fn eval_run_role_defaults_to_turn_judge() -> anyhow::Result<()> {
        assert_eq!(Run::try_parse_from(["x"])?.args.role, "turn_judge");
        let session = Run::try_parse_from(["x", "--role", "session_judge", "--bundle", "9"])?.args;
        assert_eq!(
            (session.role.as_str(), session.bundle),
            ("session_judge", Some(9))
        );
        Ok(())
    }

    #[test]
    fn eval_run_bundle_keeps_label_options_and_default_builder() -> anyhow::Result<()> {
        let parsed = Run::try_parse_from([
            "x",
            "--bundle",
            "14",
            "--split",
            "holdout",
            "--max-usd",
            "0.5",
        ])?
        .args;
        assert_eq!(parsed.bundle, Some(14));
        assert_eq!(parsed.split, "holdout");
        assert_eq!(parsed.builder, "judge-input-v2.1");
        Ok(())
    }

    #[test]
    fn eval_run_bundle_rejects_component_overrides_and_resume() {
        for (flag, value) in [
            ("--rubric", "r3"),
            ("--builder", "judge-input-v2.1"),
            ("--slices", "turnfacts"),
            ("--prompt-file", "prompt.txt"),
            ("--model", "other-model"),
            ("--resume", "3"),
        ] {
            let result = Run::try_parse_from(["x", "--bundle", "14", flag, value]);
            assert!(
                result.is_err(),
                "{flag} must not override the selected bundle"
            );
        }
    }

    #[test]
    fn eval_show_parses_run_or_gate_id() -> anyhow::Result<()> {
        assert_eq!(Show::try_parse_from(["x", "12"])?.args.id, 12);
        assert!(Show::try_parse_from(["x", "12", "--gate"])?.args.gate);
        Ok(())
    }
}
