use super::*;

#[test]
fn test_insert_and_get() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let req = make_test_request(None, "sess-1");
    let id = req.id;

    store.insert(req);

    let retrieved = store.get(&id).ok_or_else(|| anyhow::anyhow!("not found"))?;
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.session_id, "sess-1");
    assert_eq!(retrieved.method, "POST");
    assert_eq!(store.len(), 1);

    Ok(())
}

#[test]
fn test_prune_removes_old_complete() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(100));

    let mut req1 = make_test_request(None, "sess-1");
    let id1 = req1.id;
    req1.created_at = Utc::now() - chrono::Duration::hours(2);
    store.insert(req1);

    let req2 = make_test_request(None, "sess-1");
    let id2 = req2.id;
    store.insert(req2);

    let mut req3 = make_test_request(None, "sess-1");
    let id3 = req3.id;
    req3.status = RequestStatus::Streaming;
    req3.created_at = Utc::now() - chrono::Duration::hours(2);
    store.insert(req3);

    assert_eq!(store.len(), 3);

    let cutoff = Utc::now() - chrono::Duration::hours(1);
    let pruned = store.prune_before(cutoff);

    assert_eq!(pruned, 1);
    assert_eq!(store.len(), 2);
    assert!(store.get(&id1).is_none(), "old complete should be pruned");
    assert!(store.get(&id2).is_some(), "recent should survive");
    assert!(store.get(&id3).is_some(), "streaming should survive prune");

    Ok(())
}

#[test]
fn test_update_returns_false_if_missing() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    let fake_id = RequestId::from_raw(99999);
    let result = store.update(&fake_id, |req| {
        req.status = RequestStatus::Complete;
    });

    assert!(!result, "update of missing ID should return false");

    Ok(())
}
