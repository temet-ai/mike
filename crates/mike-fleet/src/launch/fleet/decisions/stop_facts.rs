use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::check::{asker, is_running};
use super::super::commit::covers;
use super::super::definition::FleetDefinition;
use super::super::events::{EventKind, FleetEvent};
use super::super::seated::Seated;
use super::text;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::mailbox::MailboxMessageWithActivity;

const CHANGES_READ: i64 = 500;
const MAIL_PREVIEW_BYTES: usize = 160;
const MAIL_LISTED: usize = 3;
const PENDING_CHECK_MINUTES: i64 = 30;

static REFUSALS: LazyLock<DashMap<Uuid, (DateTime<Utc>, String)>> = LazyLock::new(DashMap::new);

pub fn note_report_refusal(session: Uuid, reason: &str) {
    REFUSALS.insert(session, (Utc::now(), reason.to_owned()));
}

fn refusal_after(session: Uuid, last_report_at: Option<DateTime<Utc>>) -> Option<String> {
    let entry = REFUSALS.get(&session)?;
    let (at, reason) = entry.value();
    last_report_at
        .is_none_or(|report| *at > report)
        .then(|| reason.clone())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastCheck {
    pub passed: bool,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StopFacts {
    pub seat: String,
    pub task: String,
    pub uncommitted: Vec<String>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub check_passed_after_change: bool,
    pub last_check: Option<LastCheck>,
    pub open_asks: Vec<String>,
    pub check_running: bool,
    pub done_when: Option<Value>,
    pub last_commit_sha: Option<String>,
    pub last_commit_at: Option<DateTime<Utc>>,
    pub last_report_at: Option<DateTime<Utc>>,
    pub report_filed_awaiting_verdict: bool,
    pub last_report_refusal: Option<String>,
}

impl StopFacts {
    pub fn unchecked_changes(&self) -> bool {
        !self.uncommitted.is_empty() && !self.check_passed_after_change
    }

    pub fn worked_since(&self, since: DateTime<Utc>) -> bool {
        [
            self.last_change_at,
            self.last_commit_at,
            self.last_report_at,
            self.last_check.as_ref().and_then(|check| check.finished_at),
        ]
        .into_iter()
        .flatten()
        .any(|at| at > since)
    }

    pub fn state(&self, final_message: &str) -> Value {
        json!({
            "task": self.task,
            "done_when": self.done_when,
            "uncommitted_files": self.uncommitted.len(),
            "check_passed_after_last_change": self.check_passed_after_change,
            "a_check_is_running": self.check_running,
            "open_questions_and_mail_the_seat_sent": self.open_asks,
            "last_commit_sha": self.last_commit_sha,
            "last_commit_at": self.last_commit_at,
            "a_report_is_filed_and_awaits_its_verdict": self.report_filed_awaiting_verdict,
            "last_report_refusal": self.last_report_refusal,
            "final_message": final_message,
        })
    }
}

fn last_check_of(row: Option<crate::store::persistence::fleet::TreeCheckRow>) -> Option<LastCheck> {
    row.filter(|r| r.finished_at.is_some()).map(|r| LastCheck {
        passed: r.passed(),
        finished_at: r.finished_at,
    })
}

pub fn passed_after(check: Option<&LastCheck>, change: Option<DateTime<Utc>>) -> bool {
    match (check, change) {
        (Some(check), Some(change)) => {
            check.passed && check.finished_at.is_some_and(|done| done >= change)
        }
        (Some(check), None) => check.passed,
        (None, _) => false,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskRecord {
    pub last_commit_sha: Option<String>,
    pub last_commit_at: Option<DateTime<Utc>>,
    pub last_report_at: Option<DateTime<Utc>>,
    pub report_awaiting_verdict: bool,
}

fn note_of(event: &FleetEvent) -> Option<&str> {
    event.payload.get("note_uuid").and_then(Value::as_str)
}

pub fn task_record(events: &[FleetEvent]) -> TaskRecord {
    let newest = |kind: EventKind| {
        events
            .iter()
            .filter(|event| event.kind == kind)
            .max_by_key(|event| (event.occurred_at, event.id))
    };
    let commit = newest(EventKind::CommitLanded);
    let report = newest(EventKind::ReportFiled);
    let judged = |note: &str| {
        events
            .iter()
            .any(|event| event.kind == EventKind::VerdictRecorded && note_of(event) == Some(note))
    };
    TaskRecord {
        last_commit_sha: commit
            .and_then(|event| event.payload.get("sha"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        last_commit_at: commit.map(|event| event.occurred_at),
        last_report_at: report.map(|event| event.occurred_at),
        report_awaiting_verdict: report
            .is_some_and(|event| note_of(event).is_none_or(|note| !judged(note))),
    }
}

pub fn unanswered_mail(
    (outbox, inbox): (&[MailboxMessageWithActivity], &[MailboxMessageWithActivity]),
    control: &[Uuid],
) -> Vec<String> {
    let answered = |sent: &MailboxMessageWithActivity| {
        inbox.iter().any(|reply| {
            reply.row.from_session_uuid == Some(sent.row.to_session_uuid)
                && (reply.row.in_reply_to_uuid == Some(sent.row.message_uuid)
                    || reply.row.queued_at > sent.row.queued_at)
        })
    };
    let open: Vec<String> = outbox
        .iter()
        .filter(|sent| {
            control.contains(&sent.row.to_session_uuid) && sent.row.dead_lettered_at.is_none()
        })
        .filter(|sent| !answered(sent))
        .map(|sent| {
            format!(
                "mail to the coordinator or acceptance seat, unanswered: {}",
                text::head(&sent.row.body, MAIL_PREVIEW_BYTES)
            )
        })
        .collect();
    let skip = open.len().saturating_sub(MAIL_LISTED);
    open.into_iter().skip(skip).collect()
}

fn control_sessions(def: &FleetDefinition, session: Uuid) -> Vec<Uuid> {
    [&def.coordinator, &def.acceptance]
        .into_iter()
        .filter_map(|seat| seat.session.as_deref()?.trim().parse::<Uuid>().ok())
        .filter(|control| *control != session)
        .collect()
}

async fn open_asks(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    session: Uuid,
) -> Vec<String> {
    let mut asks: Vec<String> = match persist.launch().open_questions_in_run(run).await {
        Ok(rows) => rows
            .into_iter()
            .filter(|q| q.asker == session)
            .map(|q| q.question)
            .collect(),
        Err(_) => Vec::new(),
    };
    if let Ok(mail) = persist
        .launch()
        .fetch_for_session_with_activity(session)
        .await
    {
        let control = control_sessions(def, session);
        asks.extend(unanswered_mail((&mail.outbox, &mail.inbox), &control));
    }
    asks
}

pub fn check_pending(row: Option<&TreeCheckRow>, now: DateTime<Utc>) -> bool {
    row.is_some_and(|row| {
        row.finished_at.is_none() && row.started_at > now - Duration::minutes(PENDING_CHECK_MINUTES)
    })
}

async fn last_change(
    persist: &PersistHandle,
    run: &str,
    dirty: &[String],
) -> Option<DateTime<Utc>> {
    let changes = persist
        .fleet()
        .tree_changes_since(run, DateTime::<Utc>::MIN_UTC, None, CHANGES_READ)
        .await
        .ok()?;
    changes
        .iter()
        .filter(|c| dirty.contains(&c.path))
        .map(|c| c.seen_at)
        .max()
}

pub async fn gather(
    persist: &PersistHandle,
    (def, seated): (&FleetDefinition, &Seated),
    run: &str,
    session: Uuid,
) -> Option<StopFacts> {
    let task = seated.task_of(session)?.to_owned();
    let who = asker(def, seated, session)?;
    let dirty: Vec<String> = super::super::commit::changed_paths(&def.repo)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|path| who.everything || covers(&who.owned, path))
        .collect();
    let last_change_at = last_change(persist, run, &dirty).await;
    let latest = persist.fleet().latest_tree_check(run).await.ok().flatten();
    let pending = check_pending(latest.as_ref(), Utc::now());
    let last_check = last_check_of(latest);
    let record = match persist.fleet().fleet_events_for_task(run, &task).await {
        Ok(events) => task_record(&events),
        Err(_) => TaskRecord::default(),
    };
    let done_when = def
        .tasks
        .get(&task)
        .and_then(|t| t.done_when.as_ref())
        .and_then(|d| serde_json::to_value(d).ok());
    Some(StopFacts {
        seat: who.seat,
        check_passed_after_change: passed_after(last_check.as_ref(), last_change_at),
        task,
        uncommitted: dirty,
        last_change_at,
        last_check,
        open_asks: open_asks(persist, def, run, session).await,
        check_running: is_running(run) || pending,
        done_when,
        last_commit_sha: record.last_commit_sha,
        last_commit_at: record.last_commit_at,
        last_report_at: record.last_report_at,
        report_filed_awaiting_verdict: record.report_awaiting_verdict,
        last_report_refusal: refusal_after(session, record.last_report_at),
    })
}

#[cfg(test)]
#[path = "stop_facts_tests.rs"]
mod tests;
