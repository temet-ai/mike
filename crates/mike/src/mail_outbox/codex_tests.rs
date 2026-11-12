use serde_json::json;
use uuid::Uuid;

use super::claude::found_in;
use super::codex::{classify, reach_of, sighting_in};
use super::envelope::{envelope, marker};
use super::mailbox_link::MailboxState;
use super::{Found, Reach, Sent, Stage};
use crate::launch::codex::CodexError;

#[test]
fn mail_outbox_reach_reads_idle_and_the_in_progress_turn() {
    assert_eq!(
        reach_of(&json!({"status": {"type": "idle"}, "turns": []})),
        Reach::Idle
    );
    let busy = json!({"status": {"type": "active", "activeFlags": []}, "turns": [
        {"id": "t1", "status": "completed"}, {"id": "t2", "status": "inProgress"}]});
    assert_eq!(
        reach_of(&busy),
        Reach::Busy {
            turn_id: "t2".into()
        }
    );
    let lost = json!({"status": {"type": "active"}, "turns": []});
    assert!(matches!(reach_of(&lost), Reach::Unreachable(_)));
    assert!(matches!(
        reach_of(&json!({"status": {"type": "systemError"}})),
        Reach::Unreachable(_)
    ));
}

#[test]
fn mail_outbox_sighting_matches_client_id_or_marker_and_reports_each_stage() -> anyhow::Result<()> {
    let message = Uuid::new_v4();
    let by_id = json!({"turns": [{"id": "t1", "status": "completed", "items": [
        {"type": "userMessage", "id": "u1", "clientId": message.to_string(), "content": []},
        {"type": "commandExecution", "id": "c1"}]}]});
    let Found::Seen(seen) = sighting_in(&by_id, message) else {
        anyhow::bail!("not seen");
    };
    assert_eq!(seen.turn_id.as_deref(), Some("t1"));
    let stages: Vec<_> = seen.stages.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        stages,
        vec![
            Stage::Accepted,
            Stage::Consumed,
            Stage::ActionStarted,
            Stage::Completed
        ]
    );

    let by_text = json!({"turns": [{"id": "t2", "status": "inProgress", "items": [
        {"type": "userMessage", "id": "u2", "content": [{"type": "text", "text": envelope(message, None, None, "hi")}]}]}]});
    let Found::Seen(only_consumed) = sighting_in(&by_text, message) else {
        anyhow::bail!("not seen by marker");
    };
    assert!(only_consumed.has(Stage::Consumed));
    assert!(!only_consumed.has(Stage::ActionStarted));
    assert!(!only_consumed.has(Stage::Completed));
    Ok(())
}

#[test]
fn mail_outbox_absent_message_is_not_seen_unless_the_read_was_partial() {
    let message = Uuid::new_v4();
    let full =
        json!({"turns": [{"id": "t", "status": "completed", "itemsView": "full", "items": []}]});
    assert_eq!(sighting_in(&full, message), Found::NotSeen);
    let partial = json!({"turns": [{"id": "t", "itemsView": "summary", "items": []}]});
    assert!(matches!(sighting_in(&partial, message), Found::Unknown(_)));
    assert!(matches!(
        sighting_in(&json!({}), message),
        Found::Unknown(_)
    ));
}

#[test]
fn mail_outbox_transport_errors_split_into_before_send_after_send_turn_ended_and_refused() {
    let rpc = |error: &str| CodexError::Rpc {
        method: "turn/steer".into(),
        params: String::new(),
        error: error.into(),
    };
    assert!(matches!(
        classify(&rpc("expectedTurnId does not match the active turn")),
        Sent::TurnEnded(_)
    ));
    assert!(matches!(
        classify(&rpc("input too large")),
        Sent::Refused(_)
    ));
    assert!(matches!(
        classify(&CodexError::Closed("stdin")),
        Sent::FailedBeforeSend(_)
    ));
    assert!(matches!(
        classify(&CodexError::Closed("seat control")),
        Sent::FailedBeforeSend(_)
    ));
    assert!(matches!(
        classify(&CodexError::Timeout("turn/start".into())),
        Sent::FailedAfterSend(_)
    ));
    assert!(matches!(
        classify(&CodexError::Closed("seat reply")),
        Sent::FailedAfterSend(_)
    ));
}

#[test]
fn mail_outbox_envelope_names_sender_scope_and_peer_authority() {
    let (message, from) = (Uuid::new_v4(), Uuid::new_v4());
    let text = envelope(message, Some(from), Some("crates/x only"), "do it");
    assert!(
        text.starts_with(&format!("[{}]", marker(message))),
        "{text}"
    );
    assert!(text.contains(&format!("session {from}")));
    assert!(text.contains("Delegated scope: crates/x only"));
    assert!(text.contains("not an instruction from the operator"));
    assert!(text.ends_with("do it"));
    assert!(envelope(message, None, None, "x").contains("an unnamed sender"));
}

#[test]
fn mail_outbox_claude_mailbox_columns_map_to_stages_and_unknown_stays_unknown() -> anyhow::Result<()>
{
    assert_eq!(found_in(&MailboxState::default()), Found::NotSeen);
    let injected = MailboxState {
        claimed_at: Some(chrono::Utc::now()),
        sent_request_id: Some(7),
        answered: true,
        ..MailboxState::default()
    };
    let Found::Seen(seen) = found_in(&injected) else {
        anyhow::bail!("not seen");
    };
    let stages: Vec<_> = seen.stages.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        stages,
        vec![Stage::Accepted, Stage::Consumed, Stage::ActionStarted]
    );
    let acked = MailboxState {
        consumed_at: Some(chrono::Utc::now()),
        ..MailboxState::default()
    };
    let Found::Seen(acked) = found_in(&acked) else {
        anyhow::bail!("not seen");
    };
    assert!(acked.has(Stage::Accepted) && acked.has(Stage::Consumed));
    assert!(!acked.has(Stage::Completed));
    Ok(())
}
