use super::acceptance::Evidence;
use super::definition::{FleetDefinition, TaskDefinition, natural_order};
use super::fold::Folded;
use super::status::{CriterionStatus, SeatStatus, TaskStatus};

pub fn task_status(
    id: &str,
    task: &TaskDefinition,
    evidence: &Evidence<'_>,
    seats: &[SeatStatus],
    folded: &Folded,
) -> TaskStatus {
    let mine: Vec<_> = evidence
        .verdicts
        .iter()
        .filter(|verdict| verdict.tasks.iter().any(|task| task == id))
        .collect();
    let passes = mine
        .iter()
        .filter(|verdict| verdict.verdict == "pass")
        .count();
    let fails = mine.len() - passes;
    let last = mine.last().map(|verdict| (*verdict).clone());
    let passed = evidence.passed_criteria(id, &task.criteria);
    let complete = !task.criteria.is_empty() && passed.len() == task.criteria.len();
    let ready =
        evidence.ready(id, complete) || (!task.exports.is_empty() && folded.exports_passed(id));
    let name_of = |session: Option<uuid::Uuid>| {
        session.and_then(|wanted| {
            seats
                .iter()
                .find(|seat| seat.session == Some(wanted))
                .map(|seat| seat.seat.clone())
        })
    };
    let seat = name_of(folded.seat_of(id));
    let last_seat = name_of(folded.last_seat(id));
    let seat_open = folded.seat_of(id).is_some();
    TaskStatus {
        id: id.to_string(),
        name: task.name.clone(),
        discipline: task.discipline.clone(),
        criteria: task.criteria.clone(),
        seat,
        last_seat,
        state: task_state(Derived {
            complete,
            ready,
            failed: evidence.failed(id, &task.criteria),
            report_waiting: evidence.report_waiting(id) || folded.awaiting_verdict(id),
            seat_open,
            not_started: folded.not_started(id),
        }),
        passes,
        fails,
        rounds: folded.rounds(id),
        last,
        passed_criteria: passed,
        ready,
    }
}

struct Derived {
    complete: bool,
    ready: bool,
    failed: bool,
    report_waiting: bool,
    seat_open: bool,
    not_started: Option<String>,
}

fn task_state(derived: Derived) -> String {
    match () {
        _ if derived.failed => "rework",
        _ if derived.complete => "passed",
        _ if derived.ready => "ready",
        _ if derived.report_waiting => "awaiting acceptance",
        _ if derived.seat_open => "working",
        _ => {
            return derived.not_started.unwrap_or_else(|| "queued".to_string());
        }
    }
    .to_string()
}

pub fn criteria_status(def: &FleetDefinition, tasks: &[TaskStatus]) -> Vec<CriterionStatus> {
    let mut ids: Vec<&String> = def.criteria.keys().collect();
    ids.sort_by_key(|id| natural_order(id));
    ids.into_iter()
        .map(|id| {
            let serving: Vec<&TaskStatus> = tasks
                .iter()
                .filter(|task| task.criteria.contains(id))
                .collect();
            CriterionStatus {
                id: id.clone(),
                text: def
                    .criteria
                    .get(id)
                    .map(|criterion| criterion.text.clone())
                    .unwrap_or_default(),
                tasks: serving.len(),
                passed: serving
                    .iter()
                    .filter(|task| task.passed_criteria.contains(id))
                    .count(),
                ready: serving.iter().filter(|task| task.ready).count(),
            }
        })
        .collect()
}
