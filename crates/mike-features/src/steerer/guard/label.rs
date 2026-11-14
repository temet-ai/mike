use crate::logged::ok_or_warn;
use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use super::Question;
use super::label_call::{LabelRequest, call_labeller};
use super::label_prod::ProdAnswers;
use super::label_rules::{GUARD_LABELLER_VERSION, LabelInput, Settlement, from_answer, settle};
use super::policy::GuardSettings;
use crate::llm::{LlmCallContext, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::guard::GuardLabelCandidate;
use crate::store::persistence::steering_evaluations::EvaluationLabel;

pub const PASS_LIMIT: i64 = 200;
pub const BACKOFF_MINUTES: [u64; 4] = [5, 10, 20, 40];
pub const GIVE_UP_AFTER: u32 = 5;
pub const BREAKER_FAILURES: u32 = 5;
pub const BREAKER_COOLDOWN: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Default)]
struct RowBackoff {
    attempts: u32,
    next_at: Option<Instant>,
    last_error: String,
}

#[derive(Debug, Default)]
pub struct LabellerState {
    backoff: HashMap<i64, RowBackoff>,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    last_change: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LabellerHealth {
    pub state: &'static str,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub consecutive_failures: u32,
}

static STATE: LazyLock<RwLock<LabellerState>> =
    LazyLock::new(|| RwLock::new(LabellerState::default()));

pub fn backoff_delay(attempts: u32) -> Duration {
    let idx = usize::try_from(attempts.saturating_sub(1)).unwrap_or(usize::MAX);
    BACKOFF_MINUTES
        .get(idx)
        .map_or(Duration::from_secs(24 * 3600), |m| {
            Duration::from_secs(m * 60)
        })
}

impl LabellerState {
    pub fn breaker_allows(&self, now: Instant) -> bool {
        self.opened_at
            .is_none_or(|at| now.saturating_duration_since(at) >= BREAKER_COOLDOWN)
    }

    pub fn row_allows(&self, turn_id: i64, now: Instant) -> bool {
        self.backoff
            .get(&turn_id)
            .and_then(|b| b.next_at)
            .is_none_or(|at| now >= at)
    }

    pub fn record_failure(&mut self, turn_id: i64, error: &str, now: Instant) -> u32 {
        let entry = self.backoff.entry(turn_id).or_default();
        entry.attempts += 1;
        entry.last_error = error.to_owned();
        entry.next_at = Some(now + backoff_delay(entry.attempts));
        let attempts = entry.attempts;
        self.consecutive_failures += 1;
        if self.consecutive_failures >= BREAKER_FAILURES {
            self.opened_at = Some(now);
            self.last_change = Some(chrono::Utc::now());
        }
        attempts
    }

    pub fn record_success(&mut self, turn_id: i64) {
        self.backoff.remove(&turn_id);
        if self.opened_at.is_some() || self.consecutive_failures > 0 {
            self.last_change = Some(chrono::Utc::now());
        }
        self.consecutive_failures = 0;
        self.opened_at = None;
    }

    pub fn health(&self, now: Instant) -> LabellerHealth {
        let state = if !self.breaker_allows(now) || self.opened_at.is_some() {
            "breaker_open"
        } else if self.backoff.values().any(|b| b.attempts > 0) {
            "backing_off"
        } else {
            "ok"
        };
        LabellerHealth {
            state,
            since: self.last_change,
            consecutive_failures: self.consecutive_failures,
        }
    }
}

pub async fn labeller_health() -> LabellerHealth {
    STATE.read().await.health(Instant::now())
}

pub struct LabelPass<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub toggle_on: bool,
    pub prod: ProdAnswers,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LabelReport {
    pub candidates: usize,
    pub labelled: u64,
    pub calls: usize,
    pub waiting: usize,
}

fn verdict_fired(c: &GuardLabelCandidate) -> bool {
    c.status == "resolved_correction"
}

fn actor_primary(c: &GuardLabelCandidate) -> bool {
    c.verdict_json
        .as_ref()
        .and_then(|v| v.get("actor"))
        .and_then(|a| a.get("agent_instance_id"))
        .is_none_or(Value::is_null)
}

fn exemption(c: &GuardLabelCandidate) -> Option<String> {
    c.verdict_json
        .as_ref()
        .and_then(|v| v.get("exemption"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn evidence(c: &GuardLabelCandidate, end: Option<&str>, extra: Value, prod: Option<bool>) -> Value {
    let mut base = json!({
        "labeller_version": GUARD_LABELLER_VERSION,
        "turn_verdict_id": c.live_verdict_id,
        "session_end_label": end,
        "next_genuine_request_id": c.next_genuine_request_id,
        "next_genuine_after_secs": c.next_genuine_after_secs,
        "reaction": c.reaction,
        "prod": prod,
    });
    if let (Value::Object(map), Value::Object(more)) = (&mut base, extra) {
        map.extend(more);
    }
    base
}

fn superseded_by(
    candidates: &[GuardLabelCandidate],
    c: &GuardLabelCandidate,
) -> Option<uuid::Uuid> {
    if c.is_last {
        return None;
    }
    candidates
        .iter()
        .filter(|o| {
            o.operator_turn_id == c.operator_turn_id
                && o.question == c.question
                && o.trigger_point == c.trigger_point
        })
        .filter(|o| o.dispatched_at > c.dispatched_at)
        .max_by_key(|o| o.dispatched_at)
        .map(|o| o.uuid)
}

fn settle_candidate(
    all: &[GuardLabelCandidate],
    c: &GuardLabelCandidate,
    ends: &HashMap<i64, String>,
) -> Settlement {
    let Some(question) = c.question.as_deref().and_then(Question::parse) else {
        return Settlement::Wait;
    };
    let exempt = exemption(c);
    let input = LabelInput {
        question,
        would_fire: verdict_fired(c),
        actor_primary: actor_primary(c),
        reaction: c.reaction.as_deref(),
        session_end: ends.get(&c.operator_turn_id).map(String::as_str),
        exempt: exempt.as_deref(),
        superseded_by: superseded_by(all, c).or_else(|| (!c.is_last).then(uuid::Uuid::nil)),
        has_next_message: c.next_genuine_text.is_some(),
    };
    settle(&input)
}

async fn session_ends(
    persist: &PersistHandle,
    candidates: &[GuardLabelCandidate],
) -> HashMap<i64, String> {
    let mut ids: Vec<i64> = candidates.iter().map(|c| c.operator_turn_id).collect();
    ids.sort_unstable();
    ids.dedup();
    persist
        .eval()
        .session_end_labels_for_turns(&ids, chrono::Utc::now())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.end_turn_id, r.label))
        .collect()
}

async fn label_turn(
    pass: &LabelPass<'_>,
    steerer_id: i64,
    rows: &[&GuardLabelCandidate],
    ends: &HashMap<i64, String>,
) -> Result<Vec<EvaluationLabel>, String> {
    let first = rows.first().ok_or_else(|| "no rows".to_owned())?;
    let questions: Vec<Question> = rows
        .iter()
        .filter_map(|c| c.question.as_deref().and_then(Question::parse))
        .collect();
    let llm = pass.llm.with_call_context(LlmCallContext {
        session_id: Some(first.session_id),
        steerer_id: Some(steerer_id),
        operator_turn_id: Some(first.operator_turn_id),
    });
    let verdict = call_labeller(
        &llm,
        &LabelRequest {
            ending: first.final_assistant_text.as_deref().unwrap_or(""),
            next_message: first.next_genuine_text.as_deref().unwrap_or(""),
            correction: None,
            questions: &questions,
            want_reaction: first.reaction.is_none(),
        },
    )
    .await?;
    let end = ends.get(&first.operator_turn_id).map(String::as_str);
    let prod = pass.prod.of(first).await;
    Ok(rows
        .iter()
        .filter_map(|c| {
            let q = c.question.as_deref()?;
            let answer = verdict.questions.get(q)?;
            Some(EvaluationLabel {
                uuid: c.uuid,
                label: from_answer(verdict_fired(c), &answer.corrects_this).to_owned(),
                source: "next_message".to_owned(),
                evidence: evidence(c, end, json!({"source": "label_call", "answer": answer.corrects_this, "quote": answer.quote, "reaction": verdict.reaction.clone().or_else(|| c.reaction.clone())}), prod),
            })
        })
        .collect())
}

async fn call_budget(pass: &LabelPass<'_>, steerer_id: i64) -> i64 {
    let settings = GuardSettings::load(pass.persist).await;
    let used = pass
        .persist
        .steerer()
        .steerer_calls_today(steerer_id, "eval")
        .await
        .unwrap_or(i64::MAX);
    (settings.max_label_calls_per_day - used).max(0)
}

fn give_up(rows: &[&GuardLabelCandidate], error: &str) -> Vec<EvaluationLabel> {
    rows.iter()
        .map(|c| EvaluationLabel {
            uuid: c.uuid,
            label: "indeterminate".to_owned(),
            source: "next_message".to_owned(),
            evidence: json!({"labeller_version": GUARD_LABELLER_VERSION, "label_error": error, "attempts": GIVE_UP_AFTER}),
        })
        .collect()
}

async fn may_call(turn_id: i64, budget: i64, now: Instant) -> bool {
    let state = STATE.read().await;
    budget > 0 && state.breaker_allows(now) && state.row_allows(turn_id, now)
}

async fn label_one(
    pass: &LabelPass<'_>,
    steerer_id: i64,
    turn_id: i64,
    rows: &[&GuardLabelCandidate],
    ends: &HashMap<i64, String>,
) -> Vec<EvaluationLabel> {
    let now = Instant::now();
    match label_turn(pass, steerer_id, rows, ends).await {
        Ok(done) => {
            STATE.write().await.record_success(turn_id);
            done
        }
        Err(e) => {
            let attempts = STATE.write().await.record_failure(turn_id, &e, now);
            tracing::warn!(turn_id, attempts, error = %e, "turn guard labeller: call failed");
            if attempts >= GIVE_UP_AFTER {
                give_up(rows, &e)
            } else {
                Vec::new()
            }
        }
    }
}

async fn run_calls(
    pass: &LabelPass<'_>,
    steerer_id: i64,
    pending: BTreeMap<i64, Vec<&GuardLabelCandidate>>,
    ends: &HashMap<i64, String>,
    report: &mut LabelReport,
) -> Vec<EvaluationLabel> {
    let mut budget = call_budget(pass, steerer_id).await;
    let mut labels = Vec::new();
    for (turn_id, rows) in pending {
        if !may_call(turn_id, budget, Instant::now()).await {
            report.waiting += rows.len();
            continue;
        }
        budget -= 1;
        report.calls += 1;
        labels.extend(label_one(pass, steerer_id, turn_id, &rows, ends).await);
    }
    labels
}

async fn write_labels(persist: &PersistHandle, labels: &[EvaluationLabel]) -> u64 {
    match persist.steerer().apply_evaluation_labels(labels).await {
        Ok(n) => n,
        Err(error) => {
            tracing::warn!(%error, count = labels.len(), "turn guard labeller: label write failed");
            0
        }
    }
}

pub async fn run_pass(pass: &LabelPass<'_>) -> LabelReport {
    let mut report = LabelReport::default();
    if !pass.toggle_on {
        return report;
    }
    let Some(steerer) = ok_or_warn(
        pass.persist.steerer().guard_steerer().await,
        "guard_labeller.steerer",
    )
    .flatten() else {
        return report;
    };
    let settings = GuardSettings::load(pass.persist).await;
    let candidates = pass
        .persist
        .steerer()
        .guard_label_candidates(steerer.id, PASS_LIMIT, settings.label_live_passes)
        .await
        .unwrap_or_default();
    report.candidates = candidates.len();
    let ends = session_ends(pass.persist, &candidates).await;
    let mut labels = Vec::new();
    let mut pending: BTreeMap<i64, Vec<&GuardLabelCandidate>> = BTreeMap::new();
    for c in &candidates {
        match settle_candidate(&candidates, c, &ends) {
            Settlement::Label {
                label,
                source,
                evidence: extra,
            } => {
                let prod = pass.prod.of(c).await;
                labels.push(EvaluationLabel {
                    uuid: c.uuid,
                    label: label.to_owned(),
                    source: source.to_owned(),
                    evidence: evidence(
                        c,
                        ends.get(&c.operator_turn_id).map(String::as_str),
                        extra,
                        prod,
                    ),
                });
            }
            Settlement::NeedsCall => pending.entry(c.operator_turn_id).or_default().push(c),
            Settlement::Wait => report.waiting += 1,
        }
    }
    labels.extend(run_calls(pass, steerer.id, pending, &ends, &mut report).await);
    report.labelled = write_labels(pass.persist, &labels).await;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labeller_backs_off_per_row_after_failure() {
        let mut s = LabellerState::default();
        let now = Instant::now();
        assert!(s.row_allows(7, now));
        s.record_failure(7, "timeout", now);
        assert!(!s.row_allows(7, now + Duration::from_secs(60)));
        assert!(s.row_allows(7, now + Duration::from_secs(6 * 60)));
        assert!(s.row_allows(8, now));
        assert_eq!(backoff_delay(5), Duration::from_secs(24 * 3600));
    }

    #[test]
    fn labeller_breaker_opens_after_five_failures_and_probes() {
        let mut s = LabellerState::default();
        let now = Instant::now();
        for turn in 0..5 {
            s.record_failure(turn, "boom", now);
        }
        assert!(!s.breaker_allows(now + Duration::from_secs(60)));
        assert!(s.breaker_allows(now + BREAKER_COOLDOWN));
        s.record_success(99);
        assert!(s.breaker_allows(now));
    }

    #[test]
    fn labeller_health_reports_breaker_open() {
        let mut s = LabellerState::default();
        let now = Instant::now();
        assert_eq!(s.health(now).state, "ok");
        s.record_failure(1, "boom", now);
        assert_eq!(s.health(now).state, "backing_off");
        for turn in 2..6 {
            s.record_failure(turn, "boom", now);
        }
        assert_eq!(s.health(now).state, "breaker_open");
    }
}
