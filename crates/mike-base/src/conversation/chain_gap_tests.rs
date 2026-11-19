use bytes::Bytes;
use serde_json::{Value, json};

use crate::conversation::EarlierState;
use crate::store::RequestStore;
use crate::store::models::{RequestId, ThreadKind, ThreadLink};
use crate::store::tests::{make_test_request, small_policy};

fn answered(store: &mut RequestStore, user: &str, link: Option<ThreadLink>) -> RequestId {
    let mut req = make_test_request(None, "chain-gap");
    req.request_body = Some(Bytes::from(
        json!({"messages":[{"role":"user","content":user}]}).to_string(),
    ));
    req.sent_body = None;
    req.response_body = Some(Bytes::from(
        json!({"role":"assistant","content":format!("a-{user}")}).to_string(),
    ));
    req.thread = link;
    store.insert(req)
}

fn link(kind: ThreadKind, predecessor: Option<RequestId>, previous: &str) -> ThreadLink {
    ThreadLink {
        kind,
        previous_message_id: Some(previous.to_owned()),
        predecessor,
        predecessor_instance: None,
    }
}

fn texts(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|v| v["content"].as_str().unwrap_or("?").to_owned())
        .collect()
}

#[test]
fn earlier_turns_gap_behind_predecessor_is_partial_with_known_turns() {
    let mut store = RequestStore::new(small_policy(10));
    let after_gap = answered(
        &mut store,
        "u2",
        Some(link(ThreadKind::Continue, None, "msg_never_captured")),
    );
    let newest = answered(
        &mut store,
        "u3",
        Some(link(ThreadKind::Continue, Some(after_gap), "msg_2")),
    );
    let earlier = store.earlier_turns(Some(&link(ThreadKind::Continue, Some(newest), "msg_3")));
    assert_eq!(earlier.state(), EarlierState::Partial);
    assert!(!earlier.is_complete());
    assert_eq!(texts(&earlier.values()), vec!["u2", "a-u2", "u3", "a-u3"]);
}

#[test]
fn earlier_turns_partial_walk_is_not_cached_as_whole() {
    let mut store = RequestStore::new(small_policy(10));
    let after_gap = answered(
        &mut store,
        "u2",
        Some(link(ThreadKind::Continue, None, "msg_never_captured")),
    );
    let first = store.earlier_turns(Some(&link(ThreadKind::Continue, Some(after_gap), "msg_2")));
    let second = store.earlier_turns(Some(&link(ThreadKind::Continue, Some(after_gap), "msg_2")));
    assert_eq!(first.state(), EarlierState::Partial);
    assert_eq!(second.state(), EarlierState::Partial);
}

#[test]
fn earlier_turns_unknown_thread_kind_is_unavailable() {
    let mut store = RequestStore::new(small_policy(10));
    let first = answered(&mut store, "u1", None);
    let earlier = store.earlier_turns(Some(&link(ThreadKind::Other, Some(first), "msg_1")));
    assert_eq!(earlier.state(), EarlierState::Unavailable);
}

#[test]
fn earlier_turns_create_is_whole_body() {
    let mut store = RequestStore::new(small_policy(10));
    let earlier = store.earlier_turns(Some(&link(ThreadKind::Create, None, "")));
    assert_eq!(earlier.state(), EarlierState::WholeBody);
}

#[test]
fn begin_restore_after_failure_waits_before_retrying() {
    let mut store = RequestStore::new(small_policy(10));
    let id = RequestId::from_raw(42);
    assert!(store.begin_restore(id));
    store.end_restore(id);
    store.restore_failed(id);
    assert!(!store.begin_restore(id));
}
