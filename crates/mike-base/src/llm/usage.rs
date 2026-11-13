use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmCallOutcome {
    Ok,
    HttpError,
    Timeout,
    Truncated,
    TransportError,
    RefusedCeiling,
    RefusedRate,
    RefusedSpend,
}

impl LlmCallOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::HttpError => "http_error",
            Self::Timeout => "timeout",
            Self::Truncated => "truncated",
            Self::TransportError => "transport_error",
            Self::RefusedCeiling => "refused_ceiling",
            Self::RefusedRate => "refused_rate",
            Self::RefusedSpend => "refused_spend",
        }
    }
}

pub const ROLE_COUNT: usize = 7;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LlmRole {
    Steerer,
    SessionJudge,
    JudgeWorker,
    LessonRelations,
    Eval,
    LessonProposer,
    #[default]
    Unattributed,
}

impl LlmRole {
    pub const ALL: [LlmRole; ROLE_COUNT] = [
        Self::Steerer,
        Self::SessionJudge,
        Self::JudgeWorker,
        Self::LessonRelations,
        Self::Eval,
        Self::LessonProposer,
        Self::Unattributed,
    ];

    pub const fn slot(self) -> usize {
        match self {
            Self::Steerer => 0,
            Self::SessionJudge => 1,
            Self::JudgeWorker => 2,
            Self::LessonRelations => 3,
            Self::Eval => 4,
            Self::LessonProposer => 5,
            Self::Unattributed => 6,
        }
    }

    pub fn index(self) -> usize {
        self.slot()
    }

    pub fn as_db_value(self) -> Option<&'static str> {
        match self {
            Self::Steerer => Some("steerer"),
            Self::SessionJudge => Some("session_judge"),
            Self::JudgeWorker => Some("judge_worker"),
            Self::LessonRelations => Some("lesson_relations"),
            Self::Eval => Some("eval"),
            Self::LessonProposer => Some("lesson_proposer"),
            Self::Unattributed => None,
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|role| role.as_db_value() == Some(value))
    }

    pub fn label(self) -> &'static str {
        self.as_db_value().unwrap_or("unattributed")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RoleSpend([f64; ROLE_COUNT]);

impl RoleSpend {
    pub fn of(&self, role: LlmRole) -> f64 {
        self.0.get(role.index()).copied().unwrap_or(0.0)
    }

    pub fn add(&mut self, role: LlmRole, usd: f64) {
        if let Some(slot) = self.0.get_mut(role.index()) {
            *slot += usd;
        }
    }

    pub fn total(&self) -> f64 {
        self.0.iter().sum()
    }
}

#[derive(Debug)]
pub struct DailySpendLedger {
    today: Mutex<(chrono::NaiveDate, RoleSpend)>,
}

impl DailySpendLedger {
    pub fn seeded(seed: RoleSpend) -> Self {
        Self {
            today: Mutex::new((chrono::Utc::now().date_naive(), seed)),
        }
    }

    pub fn add(&self, role: LlmRole, usd: f64) {
        self.add_on(chrono::Utc::now().date_naive(), role, usd);
    }

    pub fn snapshot(&self) -> RoleSpend {
        self.snapshot_on(chrono::Utc::now().date_naive())
    }

    fn add_on(&self, day: chrono::NaiveDate, role: LlmRole, usd: f64) {
        let mut guard = self.today.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0 != day {
            *guard = (day, RoleSpend::default());
        }
        guard.1.add(role, usd);
    }

    fn snapshot_on(&self, day: chrono::NaiveDate) -> RoleSpend {
        let mut guard = self.today.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0 != day {
            *guard = (day, RoleSpend::default());
        }
        guard.1
    }
}

#[derive(Debug, Clone)]
pub struct LlmCallRecord {
    pub role: LlmRole,
    pub model: String,
    pub generation_id: Option<String>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub reasoning_tokens: Option<i32>,
    pub cached_tokens: Option<i32>,
    pub cost_usd: Option<f64>,
    pub latency_ms: Option<i32>,
    pub outcome: LlmCallOutcome,
    pub http_status: Option<i32>,
    pub session_id: Option<i64>,
    pub steerer_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LlmCallContext {
    pub session_id: Option<i64>,
    pub steerer_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
}

pub trait LlmUsageSink: Send + Sync {
    fn record(&self, call: LlmCallRecord);
    fn spent_today(&self) -> RoleSpend;
}

#[derive(Debug, Default)]
pub struct NoopUsageSink;

impl LlmUsageSink for NoopUsageSink {
    fn record(&self, _call: LlmCallRecord) {}

    fn spent_today(&self) -> RoleSpend {
        RoleSpend::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 9, d).unwrap_or_default()
    }

    #[test]
    fn role_db_values_round_trip_for_every_persisted_role() {
        for role in LlmRole::ALL {
            match role.as_db_value() {
                Some(value) => assert_eq!(LlmRole::from_db_value(value), Some(role)),
                None => assert_eq!(role, LlmRole::Unattributed),
            }
        }
        assert_eq!(LlmRole::from_db_value("nonsense"), None);
    }

    #[test]
    fn role_indices_are_dense_and_unique() {
        let mut seen = [false; ROLE_COUNT];
        for role in LlmRole::ALL {
            assert!(!seen[role.index()], "index {} used twice", role.index());
            seen[role.index()] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn ledger_accumulates_per_role_and_totals() {
        let ledger = DailySpendLedger::seeded(RoleSpend::default());
        let today = chrono::Utc::now().date_naive();
        ledger.add_on(today, LlmRole::Steerer, 0.25);
        ledger.add_on(today, LlmRole::Steerer, 0.25);
        ledger.add_on(today, LlmRole::JudgeWorker, 0.1);
        let snap = ledger.snapshot_on(today);
        assert!((snap.of(LlmRole::Steerer) - 0.5).abs() < 1e-9);
        assert!((snap.of(LlmRole::JudgeWorker) - 0.1).abs() < 1e-9);
        assert!((snap.of(LlmRole::SessionJudge)).abs() < 1e-9);
        assert!((snap.total() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn ledger_resets_when_the_utc_day_rolls() {
        let ledger = DailySpendLedger::seeded(RoleSpend::default());
        ledger.add_on(day(1), LlmRole::Steerer, 1.0);
        assert!((ledger.snapshot_on(day(1)).total() - 1.0).abs() < 1e-9);
        assert!(ledger.snapshot_on(day(2)).total().abs() < 1e-9);
        ledger.add_on(day(2), LlmRole::SessionJudge, 0.2);
        let snap = ledger.snapshot_on(day(2));
        assert!(snap.of(LlmRole::Steerer).abs() < 1e-9);
        assert!((snap.of(LlmRole::SessionJudge) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn seeded_ledger_starts_from_the_seed() {
        let mut seed = RoleSpend::default();
        seed.add(LlmRole::LessonRelations, 0.4);
        let ledger = DailySpendLedger::seeded(seed);
        assert!((ledger.snapshot().of(LlmRole::LessonRelations) - 0.4).abs() < 1e-9);
    }
}
