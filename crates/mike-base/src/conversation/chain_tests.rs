use bytes::Bytes;
use serde_json::{Value, json};

use crate::conversation::EarlierState;
use crate::store::RequestStore;
use crate::store::models::{RequestId, ThreadKind, ThreadLink};
use crate::store::tests::{make_test_request, small_policy};

fn answered(
    store: &mut RequestStore,
    body: Value,
    reply: Value,
    link: Option<ThreadLink>,
) -> RequestId {
    let mut req = make_test_request(None, "chain");
    req.request_body = Some(Bytes::from(body.to_string()));
    req.sent_body = None;
    req.response_body = Some(Bytes::from(reply.to_string()));
    req.thread = link;
    store.insert(req)
}

fn continues(predecessor: RequestId) -> ThreadLink {
    ThreadLink {
        kind: ThreadKind::Continue,
        previous_message_id: Some("msg".to_string()),
        predecessor: Some(predecessor),
        predecessor_instance: None,
    }
}

fn texts(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|v| v["content"].as_str().unwrap_or("?").to_string())
        .collect()
}

#[test]
fn earlier_turns_walks_two_links_and_includes_each_reply() {
    let mut store = RequestStore::new(small_policy(10));
    let first = answered(
        &mut store,
        json!({"messages":[{"role":"user","content":"u1"}]}),
        json!({"role":"assistant","content":"a1"}),
        None,
    );
    let second = answered(
        &mut store,
        json!({"messages":[{"role":"user","content":"u2"}]}),
        json!({"role":"assistant","content":"a2"}),
        Some(continues(first)),
    );
    let earlier = store.earlier_turns(Some(&continues(second)));
    assert_eq!(earlier.state(), EarlierState::Chain);
    assert_eq!(texts(&earlier.values()), vec!["u1", "a1", "u2", "a2"]);
}

#[test]
fn earlier_turns_predecessor_not_in_memory_is_unavailable() {
    let mut store = RequestStore::new(small_policy(10));
    let earlier = store.earlier_turns(Some(&continues(RequestId::from_raw(999_999))));
    assert_eq!(earlier.state(), EarlierState::Unavailable);
}

#[test]
fn earlier_turns_without_link_is_whole_body() {
    let mut store = RequestStore::new(small_policy(10));
    assert_eq!(store.earlier_turns(None).state(), EarlierState::WholeBody);
}

#[test]
fn earlier_turns_predecessor_without_response_is_unavailable() {
    let mut store = RequestStore::new(small_policy(10));
    let mut req = make_test_request(None, "chain");
    req.request_body = Some(Bytes::from_static(br#"{"messages":[]}"#));
    req.response_body = None;
    let id = store.insert(req);
    assert_eq!(
        store.earlier_turns(Some(&continues(id))).state(),
        EarlierState::Unavailable
    );
}
