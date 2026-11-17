use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use super::decide::{Costs, decide};
use super::followups::{Followups, analyze, pick_sources};
use super::qualification::qualifies;
use super::queries;
use super::report::{
    CostView, FileReport, Window, exposed_sessions, no_shell_arms, render_file_report, shell_lines,
};
use super::settings_api::{InspectApi, policy_from};
use super::shell::{ShellFollowups, shell_harm_table};
use super::stats::{per_lane, per_turn, permutation_p};
use super::types::{
    Arms, Call, Cut, Decision, FILE_READ_STAGE, FOLLOWUP_WINDOW, Gaps, Lane, SHELL_STAGE, Source,
    Usage, stamp,
};

const FILE_CUT_REASONS: [&str; 2] = ["file_read_reduced", "file_read_superseded"];

pub struct RunOptions {
    pub pool: PgPool,
    pub days: i64,
    pub apply: bool,
    pub evidence: Option<PathBuf>,
    pub inspect_url: String,
    pub state_dir: PathBuf,
}

struct Scores {
    forward: Vec<f64>,
    control: Vec<f64>,
    p: f64,
    lane_forward: Vec<f64>,
    lane_control: Vec<f64>,
    lane_p: f64,
}

impl Scores {
    fn new(
        arms: &Arms,
        cost: &BTreeMap<i64, Usage>,
        lanes: &BTreeMap<Lane, Usage>,
        turns: &BTreeMap<i64, i64>,
    ) -> Self {
        let forward = per_turn(arms, cost, turns, "forward");
        let control = per_turn(arms, cost, turns, "control");
        let lane_forward = per_lane(arms, lanes, "forward");
        let lane_control = per_lane(arms, lanes, "control");
        Self {
            p: permutation_p(&forward, &control),
            lane_p: permutation_p(&lane_forward, &lane_control),
            forward,
            control,
            lane_forward,
            lane_control,
        }
    }

    fn costs(&self) -> Costs<'_> {
        Costs {
            forward: &self.forward,
            control: &self.control,
            p: self.p,
        }
    }

    fn view(&self) -> CostView<'_> {
        CostView {
            forward: &self.forward,
            control: &self.control,
            lane_forward: &self.lane_forward,
            lane_control: &self.lane_control,
            p: self.p,
            lane_p: self.lane_p,
        }
    }
}

struct FileScorecard {
    arms: Arms,
    cuts: Vec<Cut>,
    scores: Scores,
    table: super::types::HarmTable,
    gaps: Gaps,
}

fn incomplete_sessions(cost: &BTreeMap<i64, Usage>) -> u64 {
    cost.values().filter(|usage| !usage.complete_usage).count() as u64
}

fn wire_ids(cuts: &[Cut]) -> Vec<String> {
    let wanted: std::collections::BTreeSet<&str> =
        cuts.iter().map(|cut| cut.wire_id.as_str()).collect();
    wanted.into_iter().map(str::to_string).collect()
}

async fn file_scorecard(pool: &PgPool, start: DateTime<Utc>) -> Result<FileScorecard> {
    let (arms, mixed) = queries::load_arms(pool, start, Utc::now(), FILE_READ_STAGE).await?;
    let ids: Vec<i64> = arms.keys().copied().collect();
    let cuts = queries::load_cuts(pool, start, &ids, FILE_READ_STAGE, &FILE_CUT_REASONS).await?;
    let (cost, lanes, turns) = queries::load_usage(pool, start, &ids).await?;
    let calls = queries::load_file_calls(pool, start, &ids).await?;
    let sources = pick_sources(queries::load_file_sources(pool, &wire_ids(&cuts)).await?);
    let (table, mut gaps) = analyze(&Followups {
        cuts: &cuts,
        arms: &arms,
        calls: &calls,
        sources: &sources,
        until: stamp(Utc::now()),
        window: FOLLOWUP_WINDOW,
    });
    gaps.insert("mixed_arm_sessions".to_string(), mixed);
    gaps.insert(
        "sessions_with_incomplete_usage".to_string(),
        incomplete_sessions(&cost),
    );
    let scores = Scores::new(&arms, &cost, &lanes, &turns);
    Ok(FileScorecard {
        arms,
        cuts,
        scores,
        table,
        gaps,
    })
}

async fn shell_evidence(
    pool: &PgPool,
    start: DateTime<Utc>,
    cuts: &[Cut],
) -> Result<(Vec<Source>, Vec<Call>)> {
    let ids: Vec<i64> = cuts
        .iter()
        .map(|cut| cut.who.session_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let sources = queries::load_shell_sources(pool, start, &ids, &wire_ids(cuts)).await?;
    let Some(first) = cuts.iter().map(|cut| cut.created_at).min() else {
        return Ok((sources, Vec::new()));
    };
    let calls = queries::load_shell_calls(pool, first, &ids).await?;
    Ok((sources, calls))
}

async fn shell_scorecard(pool: &PgPool, start: DateTime<Utc>) -> Result<Vec<String>> {
    let (arms, mixed) = queries::load_arms(pool, start, Utc::now(), SHELL_STAGE).await?;
    if arms.is_empty() {
        return Ok(no_shell_arms());
    }
    let ids: Vec<i64> = arms.keys().copied().collect();
    let cuts =
        queries::load_cuts(pool, start, &ids, SHELL_STAGE, &["shell_output_reduced"]).await?;
    let (cost, lanes, turns) = queries::load_usage(pool, start, &ids).await?;
    let (rows, calls) = shell_evidence(pool, start, &cuts).await?;
    let sources = pick_sources(rows);
    let (table, mut gaps) = shell_harm_table(&ShellFollowups {
        cuts: &cuts,
        arms: &arms,
        sources: &sources,
        calls: &calls,
        until: stamp(Utc::now()),
        window: FOLLOWUP_WINDOW,
    });
    gaps.insert("mixed_arm_sessions".to_string(), mixed);
    gaps.insert(
        "sessions_with_incomplete_usage".to_string(),
        incomplete_sessions(&cost),
    );
    let scores = Scores::new(&arms, &cost, &lanes, &turns);
    let window = Window {
        start: &format_instant(start),
        arms: &arms,
        exposed: exposed_sessions(&cuts, &arms),
    };
    Ok(shell_lines(&window, &scores.view(), &table, &gaps))
}

fn format_instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, false)
}

pub(super) fn read_evidence(explicit: Option<&Path>, state_dir: &Path) -> Result<Option<Value>> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => {
            let saved = state_dir.join("qualification.json");
            if !saved.is_file() {
                return Ok(None);
            }
            saved
        }
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read evidence file {}", path.display()))?;
    let evidence = serde_json::from_str(&text)
        .with_context(|| format!("evidence file {} is not valid JSON", path.display()))?;
    Ok(Some(evidence))
}

#[derive(Serialize)]
struct LogLine<'a> {
    #[serde(flatten)]
    decision: &'a Decision,
    at: String,
    applied: bool,
}

fn write_state(state_dir: &Path, text: &str, decisions: &[Decision], applied: bool) -> Result<()> {
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("could not create {}", state_dir.display()))?;
    let latest = state_dir.join("latest.md");
    std::fs::write(&latest, text)
        .with_context(|| format!("could not write {}", latest.display()))?;
    let log_path = state_dir.join("decisions.jsonl");
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("could not open {}", log_path.display()))?;
    for decision in decisions {
        let line = LogLine {
            decision,
            at: format_instant(Utc::now()),
            applied,
        };
        writeln!(log, "{}", serde_json::to_string(&line)?)
            .with_context(|| format!("could not append to {}", log_path.display()))?;
    }
    Ok(())
}

pub async fn run(options: RunOptions) -> Result<String> {
    let started = Utc::now();
    let start = started - Duration::days(options.days);
    let pool = &options.pool;
    let card = file_scorecard(pool, start).await?;
    let evidence = read_evidence(options.evidence.as_deref(), &options.state_dir)?;
    let api = InspectApi::new(&options.inspect_url)?;
    let settings = api.settings().await?;
    let policy = policy_from(&settings, api.running_build().await);
    let qualified = qualifies(evidence.as_ref(), &policy, Utc::now());
    let decisions = decide(&card.scores.costs(), &settings, qualified);
    let shell = shell_scorecard(pool, start).await?;
    if options.apply {
        for decision in &decisions {
            api.put_setting(decision).await?;
        }
    }
    let start_text = format_instant(start);
    let text = render_file_report(&FileReport {
        now: Utc::now(),
        window: Window {
            start: &start_text,
            arms: &card.arms,
            exposed: exposed_sessions(&card.cuts, &card.arms),
        },
        costs: card.scores.view(),
        table: &card.table,
        gaps: &card.gaps,
        shell: &shell,
        decisions: &decisions,
        applied: options.apply,
        qualified,
    });
    write_state(&options.state_dir, &text, &decisions, options.apply)?;
    Ok(text)
}
