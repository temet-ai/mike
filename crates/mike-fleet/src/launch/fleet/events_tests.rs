use uuid::Uuid;

use super::events::*;

fn one_of_each() -> Vec<EventPayload> {
    let id = Uuid::new_v4();
    vec![
        EventPayload::PlanVersioned(PlanVersioned {
            version: 2,
            brief_version: "09c3665cc467".into(),
        }),
        EventPayload::SeatLaunchRequested(SeatLaunchRequested { launch_id: 1 }),
        EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
        EventPayload::SeatLaunchFailed(SeatLaunchFailed {
            launch_id: 1,
            reason: "no_channel".into(),
        }),
        EventPayload::SeatFirstRequest(SeatFirstRequest { request_id: 9 }),
        EventPayload::SeatHeartbeatStale(SeatHeartbeatStale {
            silent_seconds: 600,
        }),
        EventPayload::SeatProcessGone(SeatProcessGone { launch_id: 1 }),
        EventPayload::SeatClosed(SeatClosed {
            reason: "accepted".into(),
        }),
        EventPayload::SeatWoken(SeatWoken {
            via: "remote_control".into(),
        }),
        EventPayload::CommitLanded(CommitLanded { sha: "abc".into() }),
        EventPayload::TreeBrokenBy(TreeBrokenBy {
            sha: "abc".into(),
            paths: vec!["a.rs".into()],
        }),
        EventPayload::CheckQueued(CheckQueued {
            receipt_id: 4,
            operation: "check".into(),
        }),
        EventPayload::CheckFinished(CheckFinished {
            receipt_id: 4,
            sha: "abc".into(),
            passed: true,
            state: "finished".into(),
            exit: Some(0),
        }),
        EventPayload::FloorPassed(FloorPassed { sha: "abc".into() }),
        EventPayload::FloorFailed(FloorFailed {
            sha: "abc".into(),
            check: "test".into(),
        }),
        EventPayload::ReportFiled(ReportFiled { note_uuid: id }),
        EventPayload::VerdictRecorded(VerdictRecorded {
            note_uuid: id,
            criterion: "C3".into(),
            passed: false,
            needed: String::new(),
        }),
        EventPayload::CriterionSuspect(CriterionSuspect {
            criterion: "C3".into(),
            failures: 3,
        }),
        EventPayload::QuestionAsked(QuestionAsked { question_id: id }),
        EventPayload::QuestionAnswered(QuestionAnswered { question_id: id }),
        EventPayload::ProblemOpened(ProblemOpened {
            problem_id: id,
            trigger: "5.1".into(),
            detail: String::new(),
        }),
        EventPayload::ProblemDecided(ProblemDecided {
            problem_id: id,
            decided_by: "design".into(),
            decision: "record_fault".into(),
            reason: String::new(),
            evidence_read: Vec::new(),
            minutes: 0,
        }),
        EventPayload::ProblemEscalated(ProblemEscalated {
            problem_id: id,
            reason: String::new(),
            question: String::new(),
            routed: false,
        }),
        EventPayload::StopRequested(StopRequested {
            by: "operator".into(),
        }),
        EventPayload::OperatorTouched(OperatorTouched {
            what: "answered".into(),
        }),
        EventPayload::ResourceAdmitted(ResourceAdmitted {
            queued_at: chrono::DateTime::from_timestamp_micros(1_000_000).unwrap_or_default(),
            admitted_at: chrono::DateTime::from_timestamp_micros(61_000_000).unwrap_or_default(),
            resource: "mike-rust-build".into(),
            units: 1,
        }),
    ]
}

#[test]
fn every_kind_has_a_unique_name_that_parses_back() {
    let mut names: Vec<&str> = EventKind::ALL.iter().map(|k| k.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), EventKind::ALL.len());
    for kind in EventKind::ALL {
        assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(EventKind::parse("nonsense"), None);
}

#[test]
fn every_kind_has_exactly_one_payload_and_is_allowed_by_the_migration() -> anyhow::Result<()> {
    let migration = include_str!(
        "../../../../mike-base/src/store/persistence/migrations/20261022000000_launch_fleet_events.sql"
    );
    let payloads = one_of_each();
    assert_eq!(payloads.len(), EventKind::ALL.len());
    let mut seen = Vec::new();
    for payload in &payloads {
        let kind = payload.kind()?;
        assert!(!seen.contains(&kind), "two payloads for {kind:?}");
        seen.push(kind);
        assert!(migration.contains(&format!("'{}'", kind.as_str())));
    }
    for kind in EventKind::ALL {
        assert!(seen.contains(&kind), "no payload for {kind:?}");
    }
    Ok(())
}

#[test]
fn a_payload_survives_its_stored_form() -> anyhow::Result<()> {
    for payload in one_of_each() {
        let event = FleetEvent {
            id: 1,
            run_id: "r".into(),
            task: Some("T3".into()),
            session_uuid: None,
            kind: payload.kind()?,
            payload: payload.body()?,
            occurred_at: chrono::Utc::now(),
        };
        assert_eq!(event.decode()?, payload);
    }
    Ok(())
}

#[test]
fn a_body_of_the_wrong_shape_is_refused_not_guessed() {
    let event = FleetEvent {
        id: 1,
        run_id: "r".into(),
        task: None,
        session_uuid: None,
        kind: EventKind::CommitLanded,
        payload: serde_json::json!({"nothing": 1}),
        occurred_at: chrono::Utc::now(),
    };
    assert!(event.decode().is_err());
}
