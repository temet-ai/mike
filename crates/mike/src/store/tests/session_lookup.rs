use super::*;

#[test]
fn requests_by_session_id_filters_by_session_and_sorts_ascending() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    store.insert(make_test_request(
        Some(RequestId::from_raw(102)),
        "sess-target",
    ));
    store.insert(make_test_request(
        Some(RequestId::from_raw(101)),
        "sess-target",
    ));

    store.insert(make_test_request(
        Some(RequestId::from_raw(200)),
        "sess-other",
    ));

    let ids = store.requests_by_session_id("sess-target");
    let raw: Vec<u64> = ids.iter().map(|id| id.as_u64()).collect();
    assert_eq!(
        raw,
        vec![101, 102],
        "only ids in the target session should be returned, ascending by id"
    );

    assert!(store.requests_by_session_id("does-not-exist").is_empty());

    Ok(())
}

#[test]
fn session_view_title_none_when_no_title_set() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    store.insert(make_test_request(
        Some(RequestId::from_raw(400)),
        "sess-notitle",
    ));

    let session = store
        .list_sessions()
        .into_iter()
        .find(|s| s.id == "sess-notitle")
        .ok_or_else(|| anyhow::anyhow!("session not found"))?;
    assert_eq!(
        session.title, None,
        "title must be None when set_session_title was never called"
    );
    Ok(())
}

#[test]
fn session_view_title_reflects_set_session_title() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    store.insert(make_test_request(Some(RequestId::from_raw(500)), "sess-t1"));
    store.set_session_title("sess-t1", "Optimize request polling efficiency".to_string());

    let session = store
        .list_sessions()
        .into_iter()
        .find(|s| s.id == "sess-t1")
        .ok_or_else(|| anyhow::anyhow!("session not found"))?;
    assert_eq!(
        session.title.as_deref(),
        Some("Optimize request polling efficiency"),
    );
    Ok(())
}

#[test]
fn session_view_title_last_set_wins() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    store.insert(make_test_request(Some(RequestId::from_raw(600)), "sess-t2"));
    store.set_session_title("sess-t2", "Old title".to_string());
    store.set_session_title("sess-t2", "New title".to_string());

    let session = store
        .list_sessions()
        .into_iter()
        .find(|s| s.id == "sess-t2")
        .ok_or_else(|| anyhow::anyhow!("session not found"))?;
    assert_eq!(session.title.as_deref(), Some("New title"));
    Ok(())
}

#[test]
fn session_view_title_does_not_bleed_across_sessions() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    store.insert(make_test_request(Some(RequestId::from_raw(700)), "sess-a"));
    store.insert(make_test_request(Some(RequestId::from_raw(701)), "sess-b"));
    store.set_session_title("sess-a", "Session A title".to_string());

    let sess_b = store
        .list_sessions()
        .into_iter()
        .find(|s| s.id == "sess-b")
        .ok_or_else(|| anyhow::anyhow!("sess-b not found"))?;
    assert_eq!(
        sess_b.title, None,
        "title set on sess-a must not appear on sess-b"
    );
    Ok(())
}
