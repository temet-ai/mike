use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::*;
use crate::launch::fleet::events::{EventKind, FleetEvent, NewFleetEvent};

const RUN: &str = "run-1";

fn stored(events: &[NewFleetEvent], start: DateTime<Utc>) -> anyhow::Result<Vec<FleetEvent>> {
    events
        .iter()
        .enumerate()
        .map(|(n, new)| {
            Ok(FleetEvent {
                id: n as i64 + 1,
                run_id: new.run_id.clone(),
                task: new.task.clone(),
                session_uuid: new.session_uuid,
                kind: new.payload.kind()?,
                payload: new.payload.body()?,
                occurred_at: start + Duration::seconds(n as i64),
            })
        })
        .collect()
}

fn solver(kind: DecisionKind) -> Decision {
    Decision {
        kind,
        decided_by: "problem_solver".into(),
        reason: "the cause was a missing tool in the slot".into(),
        evidence_read: vec!["receipt 1546".into()],
        question: None,
    }
}

fn kinds(events: &[NewFleetEvent]) -> anyhow::Result<Vec<EventKind>> {
    events.iter().map(|e| Ok(e.payload.kind()?)).collect()
}

#[test]
fn twelve_triggers_round_trip_by_name() {
    assert_eq!(Trigger::ALL.len(), 12);
    for trigger in Trigger::ALL {
        assert_eq!(Trigger::parse(trigger.as_str()), Some(trigger));
    }
    assert_eq!(Trigger::parse("nonsense"), None);
}

#[test]
fn the_decision_set_is_the_named_eleven() {
    assert_eq!(DecisionKind::ALL.len(), 11);
    for kind in DecisionKind::ALL {
        assert_eq!(DecisionKind::parse(kind.as_str()), Some(kind));
    }
}

#[test]
fn a_seeded_problem_is_opened_once_for_a_trigger_and_task() -> anyhow::Result<()> {
    let start = Utc::now();
    let first = open_events(
        &Ledger::default(),
        RUN,
        Trigger::RedBaseline,
        Some("R0"),
        "vitest 127",
    );
    assert!(first.fresh);
    assert_eq!(kinds(&first.events)?, vec![EventKind::ProblemOpened]);
    let ledger = Ledger::from_events(&stored(&first.events, start)?);
    let again = open_events(&ledger, RUN, Trigger::RedBaseline, Some("R0"), "vitest 127");
    assert!(!again.fresh);
    assert!(again.events.is_empty());
    assert_eq!(again.problem_id, first.problem_id);
    Ok(())
}

#[test]
fn a_decision_without_evidence_or_reason_is_refused_naming_the_field() -> anyhow::Result<()> {
    let start = Utc::now();
    let opened = open_events(&Ledger::default(), RUN, Trigger::WorkerQuestion, None, "q");
    let ledger = Ledger::from_events(&stored(&opened.events, start)?);
    let mut bare = solver(DecisionKind::RecordFault);
    bare.evidence_read.clear();
    let err = decide_events(&ledger, RUN, opened.problem_id, &bare, start).err();
    assert_eq!(err, Some(DecideError::MissingEvidence));
    let mut silent = solver(DecisionKind::RecordFault);
    silent.reason = " ".into();
    let err = decide_events(&ledger, RUN, opened.problem_id, &silent, start).err();
    assert_eq!(err, Some(DecideError::MissingReason));
    let ask = solver(DecisionKind::Escalate);
    let err = decide_events(&ledger, RUN, opened.problem_id, &ask, start).err();
    assert_eq!(err, Some(DecideError::MissingQuestion));
    let unknown = decide_events(
        &ledger,
        RUN,
        Uuid::new_v4(),
        &solver(DecisionKind::NewTask),
        start,
    );
    assert!(matches!(unknown, Err(DecideError::Unknown(_))));
    Ok(())
}

#[test]
fn a_decision_inside_the_budgets_is_recorded_without_escalating() -> anyhow::Result<()> {
    let start = Utc::now();
    let opened = open_events(
        &Ledger::default(),
        RUN,
        Trigger::RedBaseline,
        Some("R0"),
        "d",
    );
    let ledger = Ledger::from_events(&stored(&opened.events, start)?);
    let now = start + Duration::minutes(7);
    let events = decide_events(
        &ledger,
        RUN,
        opened.problem_id,
        &solver(DecisionKind::NewTask),
        now,
    )?;
    assert_eq!(kinds(&events)?, vec![EventKind::ProblemDecided]);
    Ok(())
}

#[test]
fn a_third_decision_on_one_problem_escalates() -> anyhow::Result<()> {
    let start = Utc::now();
    let opened = open_events(
        &Ledger::default(),
        RUN,
        Trigger::CriterionFailedThrice,
        Some("T5"),
        "d",
    );
    let mut log = opened.events.clone();
    let mut now = start;
    for round in 0..2 {
        let ledger = Ledger::from_events(&stored(&log, start)?);
        now += Duration::minutes(5);
        let events = decide_events(
            &ledger,
            RUN,
            opened.problem_id,
            &solver(DecisionKind::AmendRuling),
            now,
        )?;
        assert_eq!(
            kinds(&events)?,
            vec![EventKind::ProblemDecided],
            "round {round}"
        );
        log.extend(events);
    }
    let ledger = Ledger::from_events(&stored(&log, start)?);
    let third = decide_events(
        &ledger,
        RUN,
        opened.problem_id,
        &solver(DecisionKind::AmendRuling),
        now,
    )?;
    assert_eq!(
        kinds(&third)?,
        vec![EventKind::ProblemDecided, EventKind::ProblemEscalated]
    );
    log.extend(third);
    let ledger = Ledger::from_events(&stored(&log, start)?);
    let after = decide_events(
        &ledger,
        RUN,
        opened.problem_id,
        &solver(DecisionKind::NewTask),
        now,
    );
    assert_eq!(after.err(), Some(DecideError::Escalated(opened.problem_id)));
    Ok(())
}

#[test]
fn the_twenty_first_problem_of_a_run_escalates() -> anyhow::Result<()> {
    let start = Utc::now();
    let mut log: Vec<NewFleetEvent> = Vec::new();
    for n in 0..PROBLEMS_PER_RUN {
        let ledger = Ledger::from_events(&stored(&log, start)?);
        let task = format!("T{n}");
        let opening = open_events(&ledger, RUN, Trigger::StalledSeat, Some(&task), "d");
        assert_eq!(
            kinds(&opening.events)?,
            vec![EventKind::ProblemOpened],
            "problem {n}"
        );
        log.extend(opening.events);
    }
    let ledger = Ledger::from_events(&stored(&log, start)?);
    assert_eq!(ledger.opened(), PROBLEMS_PER_RUN);
    let last = open_events(&ledger, RUN, Trigger::StalledSeat, Some("T99"), "d");
    assert_eq!(
        kinds(&last.events)?,
        vec![EventKind::ProblemOpened, EventKind::ProblemEscalated]
    );
    Ok(())
}

#[test]
fn an_operator_question_escalates_at_once_and_an_old_problem_times_out() -> anyhow::Result<()> {
    let start = Utc::now();
    let opened = open_events(
        &Ledger::default(),
        RUN,
        Trigger::EnvironmentFault,
        None,
        "disk full",
    );
    let ledger = Ledger::from_events(&stored(&opened.events, start)?);
    let mut ask = solver(DecisionKind::Escalate);
    ask.question = Some("Free the Mac disk?".into());
    let events = decide_events(&ledger, RUN, opened.problem_id, &ask, start)?;
    assert_eq!(
        kinds(&events)?,
        vec![EventKind::ProblemDecided, EventKind::ProblemEscalated]
    );
    assert!(overdue_events(&ledger, RUN, start + Duration::minutes(59)).is_empty());
    let late = overdue_events(&ledger, RUN, start + Duration::minutes(61));
    assert_eq!(kinds(&late)?, vec![EventKind::ProblemEscalated]);
    Ok(())
}

struct Fixed(Decision);

#[async_trait::async_trait]
impl crate::launch::fleet::leaves::Solver for Fixed {
    async fn solve(
        &self,
        _dossier: &crate::launch::fleet::leaves::Dossier,
    ) -> Result<Decision, crate::launch::fleet::leaves::LeafError> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn a_seeded_problem_gets_a_decision_event_when_the_pass_runs() -> anyhow::Result<()> {
    let start = Utc::now();
    let (id, opened) = seed_problem(RUN, Trigger::ALL[0], Some("T4"), "red baseline");
    let events = stored(&[opened], start)?;
    let solver = Fixed(solver(DecisionKind::ALL[0]));
    let made = crate::launch::fleet::leaves::decisions_for(
        &events,
        RUN,
        start + Duration::minutes(1),
        &solver,
        &[],
    )
    .await;
    assert_eq!(kinds(&made)?, vec![EventKind::ProblemDecided]);
    assert_eq!(
        made[0].payload.body()?["problem_id"].as_str(),
        Some(id.to_string().as_str())
    );
    Ok(())
}
