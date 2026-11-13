use super::policy::STEERER_WAKE_INTERVAL;
use super::*;
use crate::store::models::{ReminderSource, SELF_IDENTITY_ID_LINE, SELF_IDENTITY_MARKER};
use crate::store::retention::RetentionPolicy;
use uuid::Uuid;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const OTHER_SESSION: &str = "22222222-2222-4222-8222-222222222222";

fn lesson(payload: &str) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Lesson,
        payload: payload.to_string(),
    }
}

fn steerer(payload: &str) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Steerer {
            kind: "verifier".to_string(),
            intervention_uuid: Uuid::new_v4().to_string(),
            advisory: false,
        },
        payload: payload.to_string(),
    }
}

fn mailbox(payload: &str) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::SessionMailbox {
            message_uuid: Uuid::new_v4(),
            from_session_uuid: None,
        },
        payload: payload.to_string(),
    }
}

#[test]
fn identical_lesson_content_to_one_session_is_delivered_once_then_suppressed() {
    let gate = InjectionGate::default();
    let reminder = lesson("five lessons, 6713 bytes");

    assert_eq!(gate.admit(SESSION, &reminder), Admission::DeliverAppendOnly);
    assert!(gate.record_delivery(SESSION, &reminder));

    assert_eq!(
        gate.admit(SESSION, &reminder),
        Admission::Suppressed(DUPLICATE_CONTENT),
        "the same lesson frame must not reach the same session twice"
    );
}

#[test]
fn different_lesson_content_to_the_same_session_is_still_delivered() {
    let gate = InjectionGate::default();
    let first = lesson("lesson set A");
    gate.record_delivery(SESSION, &first);

    assert_eq!(
        gate.admit(SESSION, &lesson("lesson set B")),
        Admission::DeliverAppendOnly
    );
}

#[test]
fn identical_lesson_content_to_a_different_session_is_still_delivered() {
    let gate = InjectionGate::default();
    let reminder = lesson("five lessons, 6713 bytes");
    gate.record_delivery(SESSION, &reminder);

    assert_eq!(
        gate.admit(OTHER_SESSION, &reminder),
        Admission::DeliverAppendOnly,
        "dedup is keyed per session, never process-wide"
    );
}

#[test]
fn a_source_kind_change_alone_does_not_collide_in_the_dedup_key() {
    let gate = InjectionGate::default();
    let payload = "same bytes, different source";
    gate.record_delivery(SESSION, &lesson(payload));

    let recall = SystemReminder {
        source: ReminderSource::Recall,
        payload: payload.to_string(),
    };
    assert_eq!(gate.admit(SESSION, &recall), Admission::DeliverAppendOnly);
}

#[test]
fn proxy_authored_sources_are_never_permitted_to_wake_a_session() {
    let gate = InjectionGate::default();
    for source in [
        ReminderSource::Lesson,
        ReminderSource::Recall,
        ReminderSource::Diagnostic,
        ReminderSource::SelfIdentity,
        ReminderSource::WorkspaceBriefing,
    ] {
        let reminder = SystemReminder {
            source,
            payload: "context".to_string(),
        };
        assert!(
            !gate.admit(SESSION, &reminder).may_wake(),
            "{:?} must take the append path, not the peer-framed push",
            reminder.source
        );
    }
}

#[test]
fn a_steerer_reminder_is_still_subject_to_its_wake_interval() {
    let gate = InjectionGate::default();
    let first = steerer("focus on the ask");
    assert_eq!(gate.admit(SESSION, &first), Admission::DeliverAndWake);
    gate.record_wake(SESSION, &first);

    let second = steerer("a different nudge entirely");
    assert_eq!(
        gate.admit(SESSION, &second),
        Admission::DeliverWakeThrottled,
        "a second wake inside the window is deferred, not suppressed"
    );
    assert!(
        gate.admit(SESSION, &second).suppression_reason().is_none(),
        "a deliberate intervention still has to be delivered, by append or queue"
    );
    assert_eq!(
        gate.admit(OTHER_SESSION, &second),
        Admission::DeliverAndWake,
        "the wake interval is per-session, not process-wide"
    );

    gate.backdate_wake(SESSION, &first, STEERER_WAKE_INTERVAL);
    assert_eq!(
        gate.admit(SESSION, &second),
        Admission::DeliverAndWake,
        "a wake older than the window no longer throttles"
    );
}

#[test]
fn repeated_steerer_content_is_never_suppressed_as_a_duplicate() {
    let gate = InjectionGate::default();
    let reminder = steerer("the very same nudge");
    for _ in 0..3 {
        assert!(
            gate.admit(SESSION, &reminder)
                .suppression_reason()
                .is_none()
        );
        gate.record_delivery(SESSION, &reminder);
    }
}

#[test]
fn repeated_mailbox_content_is_never_suppressed_as_a_duplicate() {
    let gate = InjectionGate::default();
    let reminder = mailbox("peer says hello");
    for _ in 0..3 {
        assert_eq!(gate.admit(SESSION, &reminder), Admission::DeliverAndWake);
        gate.record_delivery(SESSION, &reminder);
    }
}

#[test]
fn mailbox_has_no_wake_interval_so_consecutive_wakes_are_permitted() {
    let gate = InjectionGate::default();
    let reminder = mailbox("peer says hello");
    gate.record_wake(SESSION, &reminder);
    assert_eq!(gate.admit(SESSION, &reminder), Admission::DeliverAndWake);
}

#[test]
fn record_delivery_reports_whether_it_claimed_the_content() {
    let gate = InjectionGate::default();
    let reminder = lesson("claimed once");
    assert!(gate.record_delivery(SESSION, &reminder));
    assert!(
        !gate.record_delivery(SESSION, &reminder),
        "a repeat record must report that the claim already existed"
    );
}

#[test]
fn enqueue_if_admitted_drops_a_duplicate_instead_of_delaying_it() {
    let gate = InjectionGate::default();
    let mut store = RequestStore::new(RetentionPolicy::default());
    let reminder = lesson("five lessons, 6713 bytes");

    assert!(enqueue_if_admitted(
        &gate,
        &mut store,
        SESSION,
        reminder.clone()
    ));
    gate.record_delivery(SESSION, &reminder);

    assert!(
        !enqueue_if_admitted(&gate, &mut store, SESSION, reminder.clone()),
        "a suppressed reminder must not be queued for the next request either"
    );
    assert_eq!(store.claim_reminders(SESSION).len(), 1);
}

fn self_identity(session_id: &str) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::SelfIdentity,
        payload: format!(
            "<system-reminder source=\"{SELF_IDENTITY_MARKER}\">\n{SELF_IDENTITY_ID_LINE}{session_id}\n</system-reminder>"
        ),
    }
}

fn body_with_text(text: &str) -> String {
    serde_json::json!({"messages": [{"role": "user", "content": text}]}).to_string()
}

#[test]
fn self_identity_is_suppressed_when_its_id_line_is_already_in_the_request() {
    let gate = InjectionGate::default();
    let reminder = self_identity("s-1");
    let body = body_with_text(&reminder.payload);

    assert_eq!(
        gate.admit_for_request(SESSION, &reminder, body.as_bytes()),
        Admission::Suppressed(ALREADY_IN_REQUEST),
        "the identity block is still in context — do not repeat it"
    );
}

#[test]
fn self_identity_is_not_suppressed_by_a_quote_of_the_marker_alone() {
    let gate = InjectionGate::default();
    let reminder = self_identity("s-1");
    let body = body_with_text(&format!(
        "grep source=\"{SELF_IDENTITY_MARKER}\" in stage.rs"
    ));

    assert_eq!(
        gate.admit_for_request(SESSION, &reminder, body.as_bytes()),
        Admission::DeliverAppendOnly,
        "a request that merely mentions the marker has not been told its own id"
    );
}

#[test]
fn self_identity_is_not_suppressed_by_another_sessions_id_line() {
    let gate = InjectionGate::default();
    let reminder = self_identity("s-1");
    let body = body_with_text(&self_identity("s-2").payload);

    assert_eq!(
        gate.admit_for_request(SESSION, &reminder, body.as_bytes()),
        Admission::DeliverAppendOnly
    );
}

#[test]
fn self_identity_reinjects_once_its_id_line_drops_out_of_the_request() {
    let gate = InjectionGate::default();
    let reminder = self_identity("s-1");
    let with_line = body_with_text(&reminder.payload);

    assert_eq!(
        gate.admit_for_request(SESSION, &reminder, with_line.as_bytes()),
        Admission::Suppressed(ALREADY_IN_REQUEST)
    );
    gate.record_delivery(SESSION, &reminder);

    let compacted_away = body_with_text("no history left");
    assert_eq!(
        gate.admit_for_request(SESSION, &reminder, compacted_away.as_bytes()),
        Admission::DeliverAppendOnly,
        "once compaction drops the id line from the outbound request, identity must reassert"
    );
}

#[test]
fn plain_admit_treats_self_identity_as_absent_with_no_body_to_check() {
    let gate = InjectionGate::default();
    let reminder = self_identity("s-1");
    assert_eq!(gate.admit(SESSION, &reminder), Admission::DeliverAppendOnly);
}

#[test]
fn sweep_stale_leaves_a_map_under_the_threshold_alone() {
    let map: DashMap<String, Instant> = DashMap::new();
    for i in 0..10 {
        map.insert(
            format!("below-{i}"),
            Instant::now() - STALE_AFTER - Duration::from_secs(1),
        );
    }
    sweep_stale(&map);
    assert_eq!(map.len(), 10);
}

#[test]
fn sweep_stale_bounds_growth_past_the_threshold_and_keeps_fresh_entries() {
    let map: DashMap<String, Instant> = DashMap::new();
    for i in 0..SWEEP_THRESHOLD {
        map.insert(
            format!("stale-{i}"),
            Instant::now() - STALE_AFTER - Duration::from_secs(1),
        );
    }
    let fresh = "fresh-session".to_string();
    map.insert(fresh.clone(), Instant::now());
    assert!(map.len() > SWEEP_THRESHOLD);

    sweep_stale(&map);

    assert!(map.contains_key(&fresh));
    assert!(
        map.len() < SWEEP_THRESHOLD,
        "stale entries must be evicted once the map grows past the threshold: len={}",
        map.len()
    );
}
