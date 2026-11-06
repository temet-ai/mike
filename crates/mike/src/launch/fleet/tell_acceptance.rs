use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::status::{NoteFacts, note_facts};
use super::tell::Notice;
use crate::store::persistence::PersistHandle;

pub use super::report::Waiting;

const BURST_TASKS: usize = 2;
const WAIT_MINUTES: i64 = 5;

pub fn waiting_reports(def: &FleetDefinition, notes: &[NoteFacts]) -> Vec<Waiting> {
    super::report::waiting(def, notes)
}

pub async fn read_waiting(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> Vec<Waiting> {
    match persist.fleet_note_rows(run).await {
        Ok(rows) => {
            let notes: Vec<NoteFacts> = rows.into_iter().map(note_facts).collect();
            waiting_reports(def, &notes)
        }
        Err(err) => {
            tracing::warn!(%run, %err, "fleet: could not read the run's report notes");
            Vec::new()
        }
    }
}

pub fn queue_notice(to: Uuid, now: DateTime<Utc>, waiting: &[Waiting]) -> Option<Notice> {
    let oldest = waiting.first()?;
    if waiting.len() < BURST_TASKS || now - oldest.since < Duration::minutes(WAIT_MINUTES) {
        return None;
    }
    let lines: Vec<String> = waiting
        .iter()
        .map(|w| {
            format!(
                "- {} ({}): waiting {} min, {} report{}, newest report note {}",
                w.task,
                w.seat,
                (now - w.since).num_minutes(),
                w.reports,
                if w.reports == 1 { "" } else { "s" },
                w.newest
            )
        })
        .collect();
    let newest = waiting.iter().max_by_key(|w| w.newest_at)?.newest;
    Some(Notice {
        to,
        key: format!("acceptance-queue:{newest}"),
        covers: Vec::new(),
        body: format!(
            "[mike acceptance] {} tasks have reports waiting for your verdict, longest first:\n{}\nAnswer each with fleet_verdict (task, criteria, pass or fail, scope, reason, report_note); a newer report does not settle older obligations. Record readiness separately when dependent work can proceed. Reuse valid technical evidence, assign bounded checks for unresolved outcomes, and record final acceptance only after the required observations are complete.",
            waiting.len(),
            lines.join("\n")
        ),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::launch::fleet::tests::changed;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap_or_default()
    }

    fn def() -> FleetDefinition {
        changed(json!({
            "acceptance": {"session": Uuid::from_u128(1).to_string()},
            "tasks": {
                "T2": {"seat": "sim-T2", "session": Uuid::from_u128(12).to_string()},
                "T9": {"seat": "client-T9", "session": Uuid::from_u128(19).to_string()},
                "T4": {"seat": "sim-T4", "session": Uuid::from_u128(14).to_string()}
            }
        }))
    }

    fn note(id: u128, from: u128, kind: &str, data: serde_json::Value, at_text: &str) -> NoteFacts {
        NoteFacts {
            note_uuid: Uuid::from_u128(id),
            session_uuid: Uuid::from_u128(from),
            role: None,
            body: String::new(),
            tags: vec![],
            updated_at: at(at_text),
            kind: kind.into(),
            status: None,
            title: String::new(),
            data,
            created_at: at(at_text),
            seat: None,
        }
    }

    fn report(id: u128, from: u128, task: &str, at_text: &str) -> NoteFacts {
        note(id, from, "report", json!({"task": task}), at_text)
    }

    #[test]
    fn reports_queue_by_task_and_the_oldest_wait_comes_first() {
        let notes = vec![
            report(101, 19, "T9", "2026-10-04T10:00:00Z"),
            report(102, 12, "T2", "2026-10-04T09:50:00Z"),
            report(103, 19, "T9", "2026-10-04T10:01:00Z"),
        ];
        let waiting = waiting_reports(&def(), &notes);
        assert_eq!(waiting.len(), 2);
        assert_eq!((waiting[0].task.as_str(), waiting[0].reports), ("T2", 1));
        assert_eq!((waiting[1].task.as_str(), waiting[1].reports), ("T9", 2));
        assert_eq!(waiting[1].since, at("2026-10-04T10:00:00Z"));
        assert_eq!(waiting[1].newest, Uuid::from_u128(103));
    }

    #[test]
    fn a_report_acceptance_already_decided_is_not_waiting() {
        let notes = vec![
            report(101, 19, "T9", "2026-10-04T10:00:00Z"),
            report(102, 12, "T2", "2026-10-04T10:02:00Z"),
            note(
                201,
                1,
                "decision",
                json!({"verdict": "pass", "task": "T9", "criteria": ["C1"], "scope": "slice"}),
                "2026-10-04T10:03:00Z",
            ),
        ];
        let waiting = waiting_reports(&def(), &notes);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].task, "T2");
    }

    #[test]
    fn a_decision_by_a_worker_and_a_report_on_no_task_do_not_settle_or_count() {
        let notes = vec![
            report(101, 19, "T9", "2026-10-04T10:00:00Z"),
            report(102, 77, "T77", "2026-10-04T10:00:00Z"),
            note(
                201,
                19,
                "decision",
                json!({"verdict": "pass", "task": "T9", "criteria": ["C1"]}),
                "2026-10-04T10:03:00Z",
            ),
        ];
        let waiting = waiting_reports(&def(), &notes);
        assert_eq!(waiting.len(), 1, "{waiting:?}");
        assert_eq!(waiting[0].task, "T9");
    }

    #[test]
    fn a_burst_that_has_waited_is_mailed_with_each_wait_shown_longest_first() {
        let notes = vec![
            report(101, 19, "T9", "2026-10-04T10:00:00Z"),
            report(102, 12, "T2", "2026-10-04T09:50:00Z"),
        ];
        let waiting = waiting_reports(&def(), &notes);
        let now = at("2026-10-04T10:07:00Z");
        let Some(notice) = queue_notice(Uuid::from_u128(1), now, &waiting) else {
            panic!("two tasks waiting more than five minutes are mailed");
        };
        assert_eq!(notice.to, Uuid::from_u128(1));
        assert!(notice.key.starts_with("acceptance-queue:"));
        let (first, second) = (
            notice.body.find("T2 (sim-T2): waiting 17 min, 1 report"),
            notice.body.find("T9 (client-T9): waiting 7 min, 1 report"),
        );
        assert!(
            first.is_some() && second.is_some() && first < second,
            "{}",
            notice.body
        );
        assert!(notice.body.contains("fleet_verdict"), "{}", notice.body);
    }

    #[test]
    fn one_waiting_task_or_a_short_wait_is_not_mailed() {
        let notes = vec![
            report(101, 19, "T9", "2026-10-04T10:00:00Z"),
            report(102, 12, "T2", "2026-10-04T10:01:00Z"),
        ];
        let waiting = waiting_reports(&def(), &notes);
        let soon = at("2026-10-04T10:03:00Z");
        assert!(queue_notice(Uuid::from_u128(1), soon, &waiting).is_none());
        let late = at("2026-10-04T10:30:00Z");
        assert!(queue_notice(Uuid::from_u128(1), late, &waiting[..1]).is_none());
    }
}
