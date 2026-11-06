use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalRollupRow {
    pub id: i64,
    pub for_day: NaiveDate,
    pub sql_version: String,
    pub mode: String,
    pub labels_as_of: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub rows_written: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalRateRow {
    pub id: i64,
    pub rollup_id: i64,
    pub for_day: NaiveDate,
    pub family: String,
    pub metric: String,
    pub population: String,
    pub scope_kind: String,
    pub project_id: Option<i64>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: DateTime<Utc>,
    pub window_days: Option<i32>,
    pub role: String,
    pub bundle_id: Option<i64>,
    pub bundle_label: Option<String>,
    pub axis: String,
    pub class: String,
    pub label_tier: Option<String>,
    pub label_split: Option<String>,
    pub label_source: Option<String>,
    pub dimension: Option<String>,
    pub dimension_value: Option<String>,
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub mode: Option<String>,
    pub k: i32,
    pub n: i32,
    pub value: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub weighted: bool,
    pub n_effective: Option<f64>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct EvalAlarmRow {
    pub id: i64,
    pub kind: String,
    pub subject: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub cleared_at: Option<DateTime<Utc>>,
    pub evaluations: i32,
    pub quiet_evaluations: i32,
    pub detail: serde_json::Value,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub ack_note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RatesFilter {
    pub family: Option<String>,
    pub metric: Option<String>,
    pub population: Option<String>,
    pub role: Option<String>,
    pub bundle_id: Option<i64>,
    pub bundle_label: Option<String>,
    pub axis: Option<String>,
    pub class: Option<String>,
    pub label_tier: Option<String>,
    pub label_split: Option<String>,
    pub label_source: Option<String>,
    pub scope: Option<String>,
    pub project: Option<String>,
    pub window_days: Option<i32>,
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub dimension: Option<String>,
    pub rollup_id: Option<i64>,
    pub from_day: Option<NaiveDate>,
    pub to_day: Option<NaiveDate>,
    pub latest: bool,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlarmState {
    #[default]
    Open,
    Cleared,
    All,
}

impl AlarmState {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(Self::Open),
            "cleared" => Some(Self::Cleared),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Cleared => "cleared",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlarmsFilter {
    pub state: AlarmState,
    pub kind: Option<String>,
    pub acknowledged: Option<bool>,
    pub since: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmKind {
    LabelMixShift,
    LabelBand,
    UnjudgedBacklog,
    JudgeStalled,
    BudgetRefusals,
    ChangeWithoutGate,
    GoldLabelByAgent,
    EmbeddingDrift,
    JudgeCallLoop,
    SteerQuestionEligible,
    SteerQuestionDemoted,
    SteerHoldbackHarm,
    SteerWrongWakeCeiling,
    ShadowDisagreement,
    MetricDrop,
}

impl AlarmKind {
    pub const OPERATIONAL: [AlarmKind; 5] = [
        AlarmKind::UnjudgedBacklog,
        AlarmKind::JudgeStalled,
        AlarmKind::BudgetRefusals,
        AlarmKind::ChangeWithoutGate,
        AlarmKind::JudgeCallLoop,
    ];
    pub const DAILY: [AlarmKind; 5] = [
        AlarmKind::LabelMixShift,
        AlarmKind::LabelBand,
        AlarmKind::GoldLabelByAgent,
        AlarmKind::ShadowDisagreement,
        AlarmKind::MetricDrop,
    ];
    pub const STEER: [AlarmKind; 4] = [
        AlarmKind::SteerQuestionEligible,
        AlarmKind::SteerQuestionDemoted,
        AlarmKind::SteerHoldbackHarm,
        AlarmKind::SteerWrongWakeCeiling,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::LabelMixShift => "label_mix_shift",
            Self::LabelBand => "label_band",
            Self::UnjudgedBacklog => "unjudged_backlog",
            Self::JudgeStalled => "judge_stalled",
            Self::BudgetRefusals => "budget_refusals",
            Self::ChangeWithoutGate => "change_without_gate",
            Self::GoldLabelByAgent => "gold_label_by_agent",
            Self::EmbeddingDrift => "embedding_drift",
            Self::JudgeCallLoop => "judge_call_loop",
            Self::SteerQuestionEligible => "steer_question_eligible",
            Self::SteerQuestionDemoted => "steer_question_demoted",
            Self::SteerHoldbackHarm => "steer_holdback_harm",
            Self::SteerWrongWakeCeiling => "steer_wrong_wake_ceiling",
            Self::ShadowDisagreement => "shadow_disagreement",
            Self::MetricDrop => "metric_drop",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::OPERATIONAL.as_slice(),
            Self::DAILY.as_slice(),
            &[Self::EmbeddingDrift],
            Self::STEER.as_slice(),
        ]
        .concat()
        .into_iter()
        .find(|k| k.code() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Firing {
    pub kind: AlarmKind,
    pub subject: String,
    pub detail: serde_json::Value,
}

impl Serialize for AlarmKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct EpisodeChange {
    pub id: i64,
    pub kind: String,
    pub subject: String,
    pub opened: bool,
    pub cleared: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BacktestRow {
    pub at: DateTime<Utc>,
    pub subject: String,
    pub fires: bool,
    pub stats: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckOutcome {
    Acknowledged,
    NotFound,
    AlreadyAcknowledged,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlarmThresholds {
    pub mix_min_turns: i64,
    pub mix_min_tvd: f64,
    pub mix_min_chi2: f64,
    pub backlog_min_turns: i64,
    pub stalled_min_waiting: i64,
    pub stalled_hours: i64,
    pub refusals_min_share: f64,
    pub refusals_min_refused: i64,
    pub gate_grace_hours: i64,
    pub call_loop_min_calls: i64,
    pub call_loop_max_calls_per_row: f64,
    pub session_judge_max_attempts_per_day: i64,
    pub band_enabled: bool,
    pub band_min: f64,
    pub band_max: f64,
    pub shadow_disagreement_enabled: bool,
    pub shadow_disagreement_min_n: i64,
    pub shadow_disagreement_min_kappa: f64,
    pub metric_drop_enabled: bool,
    pub metric_drop_min_n: i64,
}
