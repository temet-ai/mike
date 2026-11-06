use std::path::PathBuf;

use chrono::{DateTime, Utc};

use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct PrintConfigArgs {
    #[arg(long, conflicts_with = "paths")]
    pub shell: bool,
    #[arg(
        long,
        help = "Print resolved installation paths as JSON without contacting services"
    )]
    pub paths: bool,
}

#[derive(Debug, Clone, Args)]
pub struct BackfillSessionPromptsArgs {
    #[arg(long, default_value_t = DEFAULT_MAX_BODY_BYTES)]
    pub max_body_bytes: i64,
}

pub(super) const DEFAULT_MAX_BODY_BYTES: i64 = 5_000_000;

#[derive(Debug, Clone, Args)]
pub struct BackfillToolDefinitionsArgs {
    #[arg(long, default_value_t = 1)]
    pub from: i64,
    #[arg(long)]
    pub to: Option<i64>,
    #[arg(long, default_value_t = DEFAULT_TOOL_BACKFILL_BATCH_SIZE)]
    pub batch_size: i64,
}

const DEFAULT_TOOL_BACKFILL_BATCH_SIZE: i64 = 500;

#[derive(Debug, Clone, Args)]
pub struct BackfillRequestEnvelopesArgs {
    #[arg(long, default_value_t = 1)]
    pub from: i64,
    #[arg(long)]
    pub to: Option<i64>,
    #[arg(long, default_value_t = DEFAULT_ENVELOPE_BACKFILL_BATCH_SIZE)]
    pub batch_size: i64,
}

const DEFAULT_ENVELOPE_BACKFILL_BATCH_SIZE: i64 = 100;

#[derive(Debug, Clone, Args)]
pub struct BackfillRequestSystemPromptsArgs {
    #[arg(long, default_value_t = DEFAULT_SYSTEM_PROMPT_BACKFILL_BATCH_SIZE)]
    pub batch_size: i64,
}

pub(super) const DEFAULT_SYSTEM_PROMPT_BACKFILL_BATCH_SIZE: i64 = 500;

#[derive(Debug, Clone, Args)]
pub struct CompactRequestBodiesArgs {
    #[arg(long, default_value_t = DEFAULT_COMPACTION_MIN_IDLE_DAYS)]
    pub min_idle_days: i32,
    #[arg(long, default_value_t = DEFAULT_COMPACTION_BATCH_SIZE)]
    pub batch_size: i64,
    #[arg(long, conflicts_with = "execute")]
    pub dry_run: bool,
    #[arg(long)]
    pub execute: bool,
}

const DEFAULT_COMPACTION_MIN_IDLE_DAYS: i32 = 14;

const DEFAULT_COMPACTION_BATCH_SIZE: i64 = 500;

#[derive(Debug, Clone, Args)]
pub struct RepairSessionPromptsArgs {
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub struct RestampSidecarOriginsArgs {
    #[arg(long, default_value_t = DEFAULT_SYSTEM_PROMPT_BACKFILL_BATCH_SIZE)]
    pub batch_size: i64,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub struct SweepOrphanedMessagesArgs {
    #[arg(long)]
    pub execute: bool,
    #[arg(long, default_value_t = 2000)]
    pub batch_size: i64,
}

#[derive(Debug, Clone, Args)]
pub struct BackfillLessonInjectionsArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, default_value_t = 1)]
    pub from: i64,
    #[arg(long)]
    pub to: Option<i64>,
}

#[derive(Debug, Clone, Args)]
pub struct ReclaimSpaceArgs {
    #[arg(long)]
    pub table: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DetectLessonRelationsArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub project: Option<String>,
}

pub(super) const DEFAULT_DOSSIER_BACKFILL_FROM: &str = "2026-07-27T00:00:00Z";

#[derive(Debug, Clone, Args)]
pub struct DossierBackfillArgs {
    #[arg(long, default_value = DEFAULT_DOSSIER_BACKFILL_FROM)]
    pub from: String,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long, value_parser = ["classify", "ledger", "facts", "entries", "steering-facts", "all"], default_value = "all")]
    pub stage: String,
    #[arg(long, default_value_t = 50)]
    pub batch_sessions: i64,
    #[arg(long)]
    pub execute: bool,
}

pub(super) const VERIFIER_REPLAY_ARMS: [&str; 3] =
    ["uncapped", "verifier-input-v1", "verifier-input-v1-wide"];

pub(super) const DEFAULT_REPLAY_MAX_USD: f64 = 15.0;

#[derive(Debug, Clone, Args)]
pub struct VerifierReplayArgs {
    #[arg(
        long,
        conflicts_with = "parity_recorded",
        required_unless_present = "parity_recorded"
    )]
    pub set: Option<PathBuf>,
    #[arg(long, value_parser = VERIFIER_REPLAY_ARMS, default_value = "verifier-input-v1")]
    pub arm: String,
    #[arg(long)]
    pub dump: Option<PathBuf>,
    #[arg(long, requires = "out")]
    pub execute: bool,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = DEFAULT_REPLAY_MAX_USD,
        help = "Stop starting new items once the recorded spend reaches this many USD, or as soon as a returned call carries no finite cost; the run can overshoot by at most one item"
    )]
    pub max_usd: f64,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub reasoning_effort: Option<String>,
    #[arg(
        long,
        conflicts_with = "parity_recorded",
        help = "Judge with the prompt in this file in place of the verifier's stored prompt, to try a wording against graded history before it goes live"
    )]
    pub prompt_file: Option<PathBuf>,
    #[arg(long, requires = "out", conflicts_with_all = ["execute", "dump"])]
    pub parity_recorded: Option<PathBuf>,
    #[arg(long, requires = "parity_recorded")]
    pub dump_parity: Option<PathBuf>,
}

pub(super) const PARITY_SOURCES: [&str; 2] = ["j2-text", "stored"];
pub(super) const PARITY_AS_OF: [&str; 2] = ["cutoff", "close"];

fn parse_timestamp(text: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(text)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| format!("not an RFC 3339 timestamp: {text} ({error})"))
}

#[derive(Debug, Clone, Args)]
pub struct DossierParityArgs {
    #[arg(long, required_unless_present = "dispositions")]
    pub against: Option<PathBuf>,
    #[arg(long, value_parser = parse_timestamp, default_value = "2026-07-27T00:00:00Z")]
    pub from: DateTime<Utc>,
    #[arg(long, value_parser = parse_timestamp, default_value = "2026-09-23T17:22:00Z")]
    pub cutoff: DateTime<Utc>,
    #[arg(long, value_parser = parse_timestamp)]
    pub horizon: Option<DateTime<Utc>>,
    #[arg(long, value_parser = PARITY_SOURCES, default_value = "j2-text")]
    pub source: String,
    #[arg(long, value_parser = PARITY_AS_OF, default_value = "cutoff")]
    pub as_of: String,
    #[arg(long, default_value = "jc01-parity")]
    pub out: PathBuf,
    #[arg(long)]
    pub max_diff: Option<usize>,
    #[arg(long, conflicts_with = "dispositions")]
    pub compare_as_of: bool,
    #[arg(long)]
    pub dispositions: bool,
}

#[derive(Debug, Clone, Args)]
pub struct OriginBackfillArgs {
    #[arg(long, value_parser = parse_timestamp, default_value = "2026-09-17T00:00:00Z")]
    pub since: DateTime<Utc>,
    #[arg(long, value_parser = parse_timestamp)]
    pub until: Option<DateTime<Utc>>,
    #[arg(long, default_value_t = 500)]
    pub batch: i64,
    #[arg(long, default_value = "jc01-origin-backfill")]
    pub out: PathBuf,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DossierRenderArgs {
    #[arg(long, required_unless_present = "e_parity")]
    pub session: Option<String>,
    #[arg(long)]
    pub as_of: Option<String>,
    #[arg(long, default_value = "knowledge")]
    pub mode: String,
    #[arg(long, default_value = "none")]
    pub audience: String,
    #[arg(long)]
    pub turn: Option<i64>,
    #[arg(long)]
    pub slices: Option<String>,
    #[arg(long)]
    pub sections: Option<String>,
    #[arg(long)]
    pub budget_chars: Option<usize>,
    #[arg(long)]
    pub full_view: bool,
    #[arg(long, conflicts_with = "session")]
    pub e_parity: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
}
