use super::*;

fn make_reminder(payload: &str) -> crate::store::models::SystemReminder {
    crate::store::models::SystemReminder {
        source: crate::store::models::ReminderSource::Diagnostic,
        payload: payload.to_string(),
    }
}

#[test]
fn enqueue_and_claim_reminders_fifo() {
    let mut store = RequestStore::new(small_policy(10));
    store.enqueue_reminder("sess-q", make_reminder("first"));
    store.enqueue_reminder("sess-q", make_reminder("second"));
    store.enqueue_reminder("sess-q", make_reminder("third"));

    let claimed = store.claim_reminders("sess-q");
    assert_eq!(claimed.len(), 3);
    assert_eq!(claimed[0].payload, "first");
    assert_eq!(claimed[1].payload, "second");
    assert_eq!(claimed[2].payload, "third");

    let claimed_again = store.claim_reminders("sess-q");
    assert!(claimed_again.is_empty(), "second claim must be empty");
}

#[test]
fn claim_reminders_on_unknown_session_returns_empty() {
    let mut store = RequestStore::new(small_policy(10));
    let claimed = store.claim_reminders("no-such-session");
    assert!(claimed.is_empty());
}

#[test]
fn remove_mailbox_reminder_drops_only_the_matching_message() {
    let mut store = RequestStore::new(small_policy(10));
    let keep_uuid = uuid::Uuid::new_v4();
    let drop_uuid = uuid::Uuid::new_v4();
    let mailbox_reminder =
        |message_uuid: uuid::Uuid, payload: &str| crate::store::models::SystemReminder {
            source: crate::store::models::ReminderSource::SessionMailbox {
                message_uuid,
                from_session_uuid: None,
            },
            payload: payload.to_string(),
        };
    store.enqueue_reminder("sess-ack", mailbox_reminder(keep_uuid, "keep"));
    store.enqueue_reminder("sess-ack", mailbox_reminder(drop_uuid, "drop"));
    store.enqueue_reminder("sess-ack", make_reminder("non-mailbox"));

    store.remove_mailbox_reminder("sess-ack", drop_uuid);

    let remaining = store.claim_reminders("sess-ack");
    assert_eq!(remaining.len(), 2, "only the acked message is removed");
    assert_eq!(remaining[0].payload, "keep");
    assert_eq!(remaining[1].payload, "non-mailbox");
}

#[test]
fn remove_mailbox_reminder_on_unknown_session_is_a_noop() {
    let mut store = RequestStore::new(small_policy(10));
    store.remove_mailbox_reminder("no-such-session", uuid::Uuid::new_v4());
    assert!(store.claim_reminders("no-such-session").is_empty());
}

#[test]
fn requeue_reminder_prepends() {
    let mut store = RequestStore::new(small_policy(10));
    store.enqueue_reminder("sess-rq", make_reminder("a"));
    store.enqueue_reminder("sess-rq", make_reminder("b"));

    let mut claimed = store.claim_reminders("sess-rq");
    assert_eq!(claimed.len(), 2);

    let failed = claimed.remove(0);
    assert_eq!(failed.payload, "a");

    store.enqueue_reminder("sess-rq", make_reminder("c"));

    store.requeue_reminder("sess-rq", failed);

    let order = store.claim_reminders("sess-rq");
    assert_eq!(order.len(), 2);
    assert_eq!(order[0].payload, "a", "requeued item must be first");
    assert_eq!(order[1].payload, "c");
}

#[test]
fn pending_reminder_survives_request_eviction() {
    let mut store = RequestStore::new(small_policy(10));
    let req = make_test_request(None, "sess-evict");
    let id = req.id;
    store.insert(req);

    store.enqueue_reminder("sess-evict", make_reminder("survive-me"));

    store.remove_entry(&id);

    let claimed = store.claim_reminders("sess-evict");
    assert_eq!(
        claimed.len(),
        1,
        "pending reminder must survive per-request eviction"
    );
    assert_eq!(claimed[0].payload, "survive-me");
}

#[test]
fn enqueue_reminder_drops_oldest_when_cap_exceeded() {
    let mut store = RequestStore::new(small_policy(10));
    let cap = crate::store::MAX_PENDING_REMINDERS_PER_SESSION;

    for i in 0..cap {
        store.enqueue_reminder("sess-cap", make_reminder(&format!("item-{i}")));
    }

    store.enqueue_reminder("sess-cap", make_reminder("overflow"));

    let claimed = store.claim_reminders("sess-cap");
    assert_eq!(claimed.len(), cap, "queue must not grow beyond the cap");
    assert_eq!(
        claimed[0].payload, "item-1",
        "oldest entry (item-0) must have been evicted"
    );
    assert_eq!(
        claimed[cap - 1].payload,
        "overflow",
        "new entry must appear at the tail"
    );
}
