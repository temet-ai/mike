use super::*;

#[test]
fn insert_new_session_creates_bucket() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let req = make_test_request(None, "sess-new");
    let id = req.id;
    store.insert(req);

    let sessions = store.list_sessions();
    assert_eq!(sessions.len(), 1, "single bucket expected");

    let view = &sessions[0];
    assert_eq!(view.id, "sess-new");
    assert_eq!(view.request_count, 1);
    assert_eq!(
        view.models,
        vec!["claude-sonnet-4-20250514".to_string()],
        "model list must carry the single request's model"
    );

    let detail = store
        .get_session("sess-new")
        .ok_or_else(|| anyhow::anyhow!("session should be present"))?;
    assert_eq!(detail.requests.len(), 1);
    assert_eq!(detail.requests[0].id, id);

    Ok(())
}

#[test]
fn insert_existing_session_appends_to_bucket() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    let mut req1 = make_test_request(None, "sess-shared");
    let id1 = req1.id;
    req1.created_at = Utc::now() - chrono::Duration::seconds(10);
    store.insert(req1);

    let mut req2 = make_test_request(None, "sess-shared");
    let id2 = req2.id;
    req2.created_at = Utc::now();
    store.insert(req2);

    let sessions = store.list_sessions();
    assert_eq!(
        sessions.len(),
        1,
        "same session_id must not spawn a second bucket"
    );
    assert_eq!(sessions[0].request_count, 2);

    let detail = store
        .get_session("sess-shared")
        .ok_or_else(|| anyhow::anyhow!("bucket should be present"))?;
    assert_eq!(detail.requests.len(), 2);
    assert_eq!(detail.requests[0].id, id1);
    assert_eq!(detail.requests[1].id, id2);

    Ok(())
}

#[test]
fn evict_one_of_many_preserves_bucket() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(2));
    let now = Utc::now();
    let old = now - ChronoDuration::hours(2);

    let req1 = make_test_request_with_timestamp(None, "sess-multi", old);
    let id1 = req1.id;
    store.insert(req1);
    let req2 = make_test_request_with_timestamp(None, "sess-multi", old);
    let id2 = req2.id;
    store.insert(req2);

    assert_eq!(store.len(), 2);
    let mid = store
        .get_session("sess-multi")
        .ok_or_else(|| anyhow::anyhow!("expected bucket"))?;
    assert_eq!(mid.requests.len(), 2);

    let req3 = make_test_request_with_timestamp(None, "sess-multi", now);
    let id3 = req3.id;
    store.insert(req3);

    assert_eq!(store.len(), 2);
    assert!(store.get(&id1).is_none(), "oldest evicted");

    let after = store
        .get_session("sess-multi")
        .ok_or_else(|| anyhow::anyhow!("bucket should still exist with 2 members"))?;
    assert_eq!(after.requests.len(), 2);
    let ids: Vec<_> = after.requests.iter().map(|r| r.id).collect();
    assert!(ids.contains(&id2) && ids.contains(&id3));
    assert!(
        !ids.contains(&id1),
        "evicted id must no longer appear in bucket"
    );
    assert_eq!(after.summary.request_count, 2);

    Ok(())
}

#[test]
fn evict_last_member_drops_bucket() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(2));
    let now = Utc::now();
    let old = now - ChronoDuration::hours(2);

    let req_a = make_test_request_with_timestamp(None, "sess-a", old);
    let id_a = req_a.id;
    store.insert(req_a);
    let req_b = make_test_request_with_timestamp(None, "sess-b", old);
    let id_b = req_b.id;
    store.insert(req_b);

    let req_c = make_test_request_with_timestamp(None, "sess-c", now);
    let id_c = req_c.id;
    store.insert(req_c);

    assert_eq!(store.len(), 2);
    assert!(store.get(&id_a).is_none(), "req_a should have been evicted");
    assert!(store.get(&id_b).is_some());
    assert!(store.get(&id_c).is_some());

    assert!(
        store.get_session("sess-a").is_none(),
        "empty bucket must be dropped, not left behind"
    );

    let session_ids: Vec<String> = store.list_sessions().into_iter().map(|c| c.id).collect();
    assert!(!session_ids.contains(&"sess-a".to_string()));
    assert!(session_ids.contains(&"sess-b".to_string()));
    assert!(session_ids.contains(&"sess-c".to_string()));

    Ok(())
}

#[test]
fn get_session_unknown_returns_none() {
    let store = RequestStore::new(small_policy(10));
    assert!(store.get_session("does-not-exist").is_none());
}

#[test]
fn list_sessions_sorted_by_last_seen_desc() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    let now = Utc::now();

    let mut req_old = make_test_request(None, "sess-old");
    req_old.created_at = now - chrono::Duration::minutes(30);
    store.insert(req_old);

    let mut req_mid = make_test_request(None, "sess-mid");
    req_mid.created_at = now - chrono::Duration::minutes(15);
    store.insert(req_mid);

    let mut req_new = make_test_request(None, "sess-new");
    req_new.created_at = now;
    store.insert(req_new);

    let sessions = store.list_sessions();
    let ids: Vec<_> = sessions.into_iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        vec![
            "sess-new".to_string(),
            "sess-mid".to_string(),
            "sess-old".to_string(),
        ],
        "sessions must be sorted by last_seen descending"
    );

    Ok(())
}

#[test]
fn list_sessions_last_status_reflects_front_of_deque_including_streaming() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    let mut older = make_test_request(None, "sess-live");
    older.created_at = Utc::now() - chrono::Duration::seconds(5);
    older.status = RequestStatus::Complete;
    store.insert(older);

    let mut newer = make_test_request(None, "sess-live");
    newer.created_at = Utc::now();
    newer.status = RequestStatus::Streaming;
    store.insert(newer);

    let sessions = store.list_sessions();
    let view = sessions
        .iter()
        .find(|c| c.id == "sess-live")
        .ok_or_else(|| anyhow::anyhow!("sess-live not present"))?;
    assert_eq!(
        view.request_count, 2,
        "both requests must be counted in the bucket"
    );
    assert_eq!(
        view.last_status, "Streaming",
        "last_status must reflect the newest inserted request, not the newest terminal one"
    );

    Ok(())
}
