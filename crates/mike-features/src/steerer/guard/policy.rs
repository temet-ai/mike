use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::RwLock;

use super::Question;
use crate::store::persistence::PersistHandle;

pub use crate::settings::meta_turn_guard::{
    AUTO_DEMOTE_KEY, FLEET_SEATS_KEY, HOLDBACK_PCT_KEY, LABEL_LIVE_PASSES_KEY,
    MAX_CALLS_PER_DAY_KEY, MAX_LABEL_CALLS_PER_DAY_KEY, RECALL_SAMPLE_PCT_KEY, WAKE_CAP_KEY,
    WAKE_GRACE_RELAPSE_KEY, WAKE_GRACE_WAIT_KEY, WRONG_WAKE_CEILING_KEY,
};

pub const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionMode {
    Off,
    Shadow,
    Append,
    Wake,
}

impl QuestionMode {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "off" => Self::Off,
            "append" => Self::Append,
            "wake" => Self::Wake,
            _ => Self::Shadow,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Shadow => "shadow",
            Self::Append => "append",
            Self::Wake => "wake",
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, Self::Append | Self::Wake)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuestionParams {
    pub confidence_threshold: f64,
    pub floor: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardConfig {
    pub questions: HashMap<Question, QuestionParams>,
    pub quiescence_after_secs: i64,
}

impl GuardConfig {
    pub fn from_json(config: &serde_json::Value) -> Self {
        let default_threshold = config
            .get("confidence_threshold")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.7);
        let questions = Question::ALL
            .into_iter()
            .map(|q| {
                let entry = config.get("questions").and_then(|qs| qs.get(q.code()));
                let get = |k: &str, d: f64| {
                    entry
                        .and_then(|e| e.get(k))
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(d)
                };
                (
                    q,
                    QuestionParams {
                        confidence_threshold: get("confidence_threshold", default_threshold),
                        floor: get("floor", 0.6),
                    },
                )
            })
            .collect();
        Self {
            questions,
            quiescence_after_secs: config
                .get("quiescence_after_secs")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(90),
        }
    }

    pub fn threshold(&self, question: Question) -> f64 {
        self.questions
            .get(&question)
            .map_or(0.7, |p| p.confidence_threshold)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GuardSettings {
    pub max_calls_per_day: i64,
    pub max_label_calls_per_day: i64,
    pub recall_sample_pct: i64,
    pub holdback_pct: i64,
    pub wake_grace_relapse_secs: i64,
    pub wake_grace_wait_secs: i64,
    pub wake_cap_per_session_hour: i64,
    pub auto_demote: bool,
    pub wrong_wake_ceiling_per_week: i64,
    pub label_live_passes: bool,
    pub fleet_seats: bool,
}

impl Default for GuardSettings {
    fn default() -> Self {
        Self {
            max_calls_per_day: 150,
            max_label_calls_per_day: 80,
            recall_sample_pct: 10,
            holdback_pct: 10,
            wake_grace_relapse_secs: 30,
            wake_grace_wait_secs: 90,
            wake_cap_per_session_hour: 3,
            auto_demote: true,
            wrong_wake_ceiling_per_week: 2,
            label_live_passes: false,
            fleet_seats: false,
        }
    }
}

fn as_bool(value: &serde_json::Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|x| x.parse().ok()))
}

impl GuardSettings {
    pub fn from_rows(rows: &[(String, serde_json::Value)]) -> Self {
        let mut s = Self::default();
        for (key, value) in rows {
            if key == AUTO_DEMOTE_KEY {
                s.auto_demote = as_bool(value).unwrap_or(s.auto_demote);
                continue;
            }
            if key == LABEL_LIVE_PASSES_KEY {
                s.label_live_passes = as_bool(value).unwrap_or(s.label_live_passes);
                continue;
            }
            if key == FLEET_SEATS_KEY {
                s.fleet_seats = as_bool(value).unwrap_or(s.fleet_seats);
                continue;
            }
            let Some(v) = value
                .as_i64()
                .or_else(|| value.as_str().and_then(|x| x.parse().ok()))
            else {
                continue;
            };
            s.apply_int(key, v);
        }
        s
    }

    fn apply_int(&mut self, key: &str, v: i64) {
        match key {
            MAX_CALLS_PER_DAY_KEY => self.max_calls_per_day = v.clamp(0, 1000),
            MAX_LABEL_CALLS_PER_DAY_KEY => self.max_label_calls_per_day = v.clamp(0, 1000),
            RECALL_SAMPLE_PCT_KEY => self.recall_sample_pct = v.clamp(0, 100),
            HOLDBACK_PCT_KEY => self.holdback_pct = v.clamp(0, 50),
            WAKE_GRACE_RELAPSE_KEY => self.wake_grace_relapse_secs = v.clamp(0, 600),
            WAKE_GRACE_WAIT_KEY => self.wake_grace_wait_secs = v.clamp(0, 900),
            WAKE_CAP_KEY => self.wake_cap_per_session_hour = v.clamp(0, 20),
            WRONG_WAKE_CEILING_KEY => self.wrong_wake_ceiling_per_week = v.clamp(0, 50),
            _ => {}
        }
    }

    pub fn grace_secs(&self, question: Question) -> i64 {
        match question {
            Question::Relapse => self.wake_grace_relapse_secs,
            _ => self.wake_grace_wait_secs,
        }
    }

    pub async fn load(persist: &PersistHandle) -> Self {
        SETTINGS_CACHE
            .get_or_load(0, Instant::now(), || async {
                persist
                    .fetch_settings()
                    .await
                    .map(|rows| Self::from_rows(&rows))
                    .unwrap_or_default()
            })
            .await
    }
}

pub struct TtlCache<V: Clone> {
    entries: RwLock<HashMap<i64, (Instant, V)>>,
    ttl: Duration,
}

impl<V: Clone> TtlCache<V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    pub async fn fresh(&self, key: i64, now: Instant) -> Option<V> {
        let entries = self.entries.read().await;
        entries
            .get(&key)
            .filter(|(at, _)| now.saturating_duration_since(*at) < self.ttl)
            .map(|(_, v)| v.clone())
    }

    pub async fn put(&self, key: i64, now: Instant, value: V) {
        self.entries.write().await.insert(key, (now, value));
    }

    pub async fn get_or_load<F, Fut>(&self, key: i64, now: Instant, load: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        if let Some(v) = self.fresh(key, now).await {
            return v;
        }
        let v = load().await;
        self.put(key, now, v.clone()).await;
        v
    }
}

static SETTINGS_CACHE: LazyLock<TtlCache<GuardSettings>> =
    LazyLock::new(|| TtlCache::new(CACHE_TTL));
static POLICY_CACHE: LazyLock<TtlCache<HashMap<Question, QuestionMode>>> =
    LazyLock::new(|| TtlCache::new(CACHE_TTL));
static FULL_POLICY_CACHE: LazyLock<TtlCache<HashMap<Question, QuestionPolicy>>> =
    LazyLock::new(|| TtlCache::new(CACHE_TTL));
static CONFIG_CACHE: LazyLock<TtlCache<GuardConfig>> = LazyLock::new(|| TtlCache::new(CACHE_TTL));

pub fn modes_from_rows(rows: &[(String, String)]) -> HashMap<Question, QuestionMode> {
    let mut modes: HashMap<Question, QuestionMode> = Question::ALL
        .into_iter()
        .map(|q| (q, QuestionMode::Shadow))
        .collect();
    for (question, mode) in rows {
        if let Some(q) = Question::parse(question) {
            modes.insert(q, QuestionMode::parse(mode));
        }
    }
    modes
}

pub async fn question_modes(
    persist: &PersistHandle,
    steerer_id: i64,
) -> HashMap<Question, QuestionMode> {
    POLICY_CACHE
        .get_or_load(steerer_id, Instant::now(), || async {
            let rows = persist
                .steerer()
                .question_policies(steerer_id)
                .await
                .unwrap_or_default();
            let pairs: Vec<(String, String)> =
                rows.into_iter().map(|r| (r.question, r.mode)).collect();
            modes_from_rows(&pairs)
        })
        .await
}

pub async fn guard_config(persist: &PersistHandle, steerer_id: i64) -> GuardConfig {
    CONFIG_CACHE
        .get_or_load(steerer_id, Instant::now(), || async {
            let config = persist
                .steerer()
                .steerer_config(steerer_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            GuardConfig::from_json(&config)
        })
        .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestionPolicy {
    pub policy_id: i64,
    pub mode: QuestionMode,
    pub holdback_pct: Option<i64>,
    pub holdback_salt: uuid::Uuid,
}

impl QuestionPolicy {
    pub fn effective_holdback(&self, settings: &GuardSettings) -> i64 {
        self.holdback_pct
            .unwrap_or(settings.holdback_pct)
            .clamp(0, 50)
    }
}

pub async fn question_policies(
    persist: &PersistHandle,
    steerer_id: i64,
) -> HashMap<Question, QuestionPolicy> {
    FULL_POLICY_CACHE
        .get_or_load(steerer_id, Instant::now(), || async {
            persist
                .steerer()
                .policy_rows(steerer_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter_map(|r| {
                    Some((
                        Question::parse(&r.question)?,
                        QuestionPolicy {
                            policy_id: r.id,
                            mode: QuestionMode::parse(&r.mode),
                            holdback_pct: r.holdback_pct.map(i64::from),
                            holdback_salt: r.holdback_salt,
                        },
                    ))
                })
                .collect()
        })
        .await
}

pub async fn forget_policies(steerer_id: i64) {
    FULL_POLICY_CACHE.entries.write().await.remove(&steerer_id);
    POLICY_CACHE.entries.write().await.remove(&steerer_id);
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn forget_all() {
    POLICY_CACHE.entries.write().await.clear();
    FULL_POLICY_CACHE.entries.write().await.clear();
    CONFIG_CACHE.entries.write().await.clear();
    SETTINGS_CACHE.entries.write().await.clear();
}

pub fn writable_mode(raw: &str) -> Result<QuestionMode, String> {
    match raw {
        "off" => Ok(QuestionMode::Off),
        "shadow" => Ok(QuestionMode::Shadow),
        "append" | "wake" => Err("promotion to append or wake goes through JC17's gate".to_owned()),
        other => Err(format!("mode {other:?} is not one of off|shadow")),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn append_and_wake_read_as_live_once_promoted() {
        let modes = modes_from_rows(&[
            ("relapse".to_owned(), "append".to_owned()),
            ("unbacked_wait".to_owned(), "wake".to_owned()),
            ("unobserved_done".to_owned(), "off".to_owned()),
        ]);
        assert_eq!(modes[&Question::Relapse], QuestionMode::Append);
        assert!(modes[&Question::UnbackedWait].is_live());
        assert_eq!(modes[&Question::UnobservedDone], QuestionMode::Off);
        assert_eq!(modes[&Question::AuthorisedHandback], QuestionMode::Shadow);
        assert!(writable_mode("wake").is_err());
        assert_eq!(writable_mode("off"), Ok(QuestionMode::Off));
    }

    #[tokio::test]
    async fn settings_cache_refreshes_after_60s() {
        let cache: TtlCache<i64> = TtlCache::new(CACHE_TTL);
        let t0 = Instant::now();
        assert_eq!(cache.get_or_load(1, t0, || async { 5 }).await, 5);
        assert_eq!(
            cache
                .get_or_load(1, t0 + Duration::from_secs(30), || async { 9 })
                .await,
            5
        );
        assert_eq!(
            cache
                .get_or_load(1, t0 + Duration::from_secs(61), || async { 9 })
                .await,
            9
        );
    }

    #[test]
    fn guard_settings_parse_rows_and_clamp() {
        let s = GuardSettings::from_rows(&[
            (MAX_CALLS_PER_DAY_KEY.to_owned(), serde_json::json!(5000)),
            (RECALL_SAMPLE_PCT_KEY.to_owned(), serde_json::json!("20")),
        ]);
        assert_eq!(s.max_calls_per_day, 1000);
        assert_eq!(s.recall_sample_pct, 20);
        assert_eq!(s.max_label_calls_per_day, 80);
        let jc17 = GuardSettings::from_rows(&[
            (HOLDBACK_PCT_KEY.to_owned(), serde_json::json!(70)),
            (AUTO_DEMOTE_KEY.to_owned(), serde_json::json!("false")),
        ]);
        assert_eq!(jc17.holdback_pct, 50);
        assert!(!jc17.auto_demote);
        assert_eq!(jc17.grace_secs(Question::Relapse), 30);
        assert_eq!(jc17.grace_secs(Question::UnbackedWait), 90);
    }

    #[test]
    fn label_live_passes_defaults_off_and_parses_true() {
        assert!(!GuardSettings::default().label_live_passes);
        let s = GuardSettings::from_rows(&[(
            LABEL_LIVE_PASSES_KEY.to_owned(),
            serde_json::json!(true),
        )]);
        assert!(s.label_live_passes);
    }
}
