use bytes::Bytes;

use super::{FollowupAnswer, RequestStore};
use crate::store::models::{RequestId, ThreadKind, ThreadLink};
use crate::store::tests::{make_test_request, small_policy};

fn answered(
    store: &mut RequestStore,
    session: &str,
    message_id: &str,
    instance: Option<i64>,
) -> RequestId {
    let mut req = make_test_request(None, session);
    req.upstream_message_id = Some(message_id.to_string());
    req.agent_instance_id = instance;
    store.insert(req)
}

fn continuing(session: &str, previous: &str) -> crate::store::models::InterceptedRequest {
    let mut req = make_test_request(None, session);
    req.request_body = Some(Bytes::from(format!(
        r#"{{"messages":[],"thread":{{"type":"continue","previous_message_id":"{previous}"}}}}"#
    )));
    req
}

fn linked(store: &RequestStore, session: &str, previous: &str) -> anyhow::Result<ThreadLink> {
    store
        .link_thread(&continuing(session, previous))
        .ok_or_else(|| anyhow::anyhow!("no thread link"))
}

#[test]
fn link_thread_continue_finds_request_answered_by_previous_id() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    answered(&mut store, "s1", "msg_a", None);
    let target = answered(&mut store, "s1", "msg_b", Some(42));
    let link = linked(&store, "s1", "msg_b")?;
    assert_eq!(link.kind, ThreadKind::Continue);
    assert_eq!(link.predecessor, Some(target));
    assert_eq!(link.predecessor_instance, Some(42));
    Ok(())
}

#[test]
fn link_thread_previous_id_in_other_session_leaves_predecessor_unset() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    answered(&mut store, "other", "msg_b", Some(7));
    let link = linked(&store, "s1", "msg_b")?;
    assert_eq!(link.predecessor, None);
    assert_eq!(link.previous_message_id.as_deref(), Some("msg_b"));
    Ok(())
}

#[test]
fn link_thread_previous_response_not_yet_seen_keeps_id_for_db_lookup() -> anyhow::Result<()> {
    let store = RequestStore::new(small_policy(10));
    let link = linked(&store, "s1", "msg_late")?;
    assert_eq!(link.predecessor, None);
    assert_eq!(link.previous_message_id.as_deref(), Some("msg_late"));
    Ok(())
}

fn followed_up(
    store: &mut RequestStore,
    original: RequestId,
    message_id: Option<&str>,
) -> RequestId {
    let mut req = make_test_request(None, "s1");
    req.upstream_message_id = message_id.map(str::to_string);
    let followup = store.insert(req);
    store.update(&original, |r| {
        r.correction_request_id = Some(followup.as_u64())
    });
    followup
}

#[test]
fn followup_answer_without_a_followup_is_not_followed_up() {
    let mut store = RequestStore::new(small_policy(10));
    answered(&mut store, "s1", "msg_a", None);
    assert_eq!(
        store.followup_answer("s1", "msg_a"),
        FollowupAnswer::NotFollowedUp
    );
}

#[test]
fn followup_answer_unknown_message_is_not_held() {
    let store = RequestStore::new(small_policy(10));
    assert_eq!(
        store.followup_answer("s1", "msg_gone"),
        FollowupAnswer::NotHeld
    );
}

#[test]
fn followup_answer_ends_at_the_last_followup_in_the_chain() {
    let mut store = RequestStore::new(small_policy(10));
    let original = answered(&mut store, "s1", "msg_a", None);
    let first = followed_up(&mut store, original, Some("msg_b"));
    followed_up(&mut store, first, Some("msg_c"));
    assert_eq!(
        store.followup_answer("s1", "msg_a"),
        FollowupAnswer::EndsAt("msg_c".to_string())
    );
}

#[test]
fn followup_answer_without_a_message_id_is_unanswered() {
    let mut store = RequestStore::new(small_policy(10));
    let original = answered(&mut store, "s1", "msg_a", None);
    followed_up(&mut store, original, None);
    assert_eq!(
        store.followup_answer("s1", "msg_a"),
        FollowupAnswer::Unanswered
    );
}

#[test]
fn link_thread_body_without_thread_is_none() {
    let store = RequestStore::new(small_policy(10));
    let mut req = make_test_request(None, "s1");
    req.request_body = Some(Bytes::from_static(br#"{"messages":[]}"#));
    assert_eq!(store.link_thread(&req), None);
}
