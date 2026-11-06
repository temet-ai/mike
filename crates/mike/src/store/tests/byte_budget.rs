use super::*;

fn small_byte_policy(max_entries: usize, max_store_bytes: u64) -> RetentionPolicy {
    RetentionPolicy {
        max_entries,
        max_age: Duration::from_secs(3600),
        prune_interval: Duration::from_secs(60),
        max_store_bytes,
    }
}

#[test]
fn evict_to_byte_budget_drops_oldest_terminal_entries_until_under_budget() -> anyhow::Result<()> {
    use bytes::Bytes;

    let mut store = RequestStore::new(small_byte_policy(100, 2500));

    for (idx, session) in ["s-1", "s-2", "s-3", "s-4"].iter().enumerate() {
        let mut req = make_test_request(Some(RequestId::from_raw(200 + idx as u64)), session);
        req.status = RequestStatus::Complete;
        req.request_body = Some(Bytes::from(vec![0u8; 1024]));
        req.created_at = Utc::now() - chrono::Duration::hours(2);
        store.insert(req);
    }

    assert!(
        store.len() <= 3,
        "expected on-insert byte sweep to keep us at <=3 entries, found {}",
        store.len()
    );

    let outcome = store.evict_to_byte_budget();
    assert!(!outcome.over_budget());
    assert_eq!(outcome.budget, 2500);
    assert!(
        outcome.bytes_remaining_after <= 2500,
        "post-sweep bytes ({}) must be <= budget ({})",
        outcome.bytes_remaining_after,
        outcome.budget
    );

    let surviving_ids: Vec<u64> = store.entries.keys().map(|id| id.as_u64()).collect();
    for id in &surviving_ids {
        assert!(
            *id >= 200 + (4 - store.len() as u64),
            "surviving id {} should be one of the most-recent inserts",
            id
        );
    }

    Ok(())
}

#[test]
fn evict_to_byte_budget_is_noop_when_disabled() -> anyhow::Result<()> {
    use bytes::Bytes;

    let mut store = RequestStore::new(small_byte_policy(100, 0));
    for idx in 0..3u64 {
        let mut req = make_test_request(Some(RequestId::from_raw(300 + idx)), "sess-d");
        req.status = RequestStatus::Complete;
        req.request_body = Some(Bytes::from(vec![0u8; 4096]));
        req.created_at = Utc::now() - chrono::Duration::hours(2);
        store.insert(req);
    }

    let before = store.len();
    let outcome = store.evict_to_byte_budget();
    assert_eq!(outcome.entries_evicted, 0);
    assert_eq!(outcome.bytes_freed, 0);
    assert_eq!(outcome.budget, 0);
    assert!(!outcome.over_budget(), "0-budget is never 'over budget'");
    assert_eq!(store.len(), before, "no entries should have been dropped");

    Ok(())
}

#[test]
fn evict_to_byte_budget_skips_pinned_sessions() -> anyhow::Result<()> {
    use bytes::Bytes;

    let mut store = RequestStore::new(small_byte_policy(100, 1024));
    store.set_session_pinned("sess-pinned", true);
    let mut req = make_test_request(Some(RequestId::from_raw(400)), "sess-pinned");
    req.status = RequestStatus::Complete;
    req.request_body = Some(Bytes::from(vec![0u8; 4096]));
    req.created_at = Utc::now() - chrono::Duration::hours(2);
    store.insert(req);

    let outcome = store.evict_to_byte_budget();
    assert_eq!(outcome.entries_evicted, 0);
    assert_eq!(outcome.bytes_freed, 0);
    assert!(
        outcome.over_budget(),
        "pinned-only store should be reported as over budget so the pruner logs WARN"
    );
    assert_eq!(store.len(), 1, "pinned entry must survive");
    Ok(())
}

#[test]
fn test_eviction_at_capacity() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(3));
    let now = Utc::now();
    let old = now - ChronoDuration::hours(2);

    let req1 = make_test_request_with_timestamp(None, "sess-1", old);
    let id1 = req1.id;
    let req2 = make_test_request_with_timestamp(None, "sess-1", old);
    let id2 = req2.id;
    let req3 = make_test_request_with_timestamp(None, "sess-1", old);
    let id3 = req3.id;

    store.insert(req1);
    store.insert(req2);
    store.insert(req3);
    assert_eq!(store.len(), 3);

    let req4 = make_test_request_with_timestamp(None, "sess-1", now);
    let id4 = req4.id;
    store.insert(req4);

    assert_eq!(store.len(), 3);
    assert!(store.get(&id1).is_none(), "oldest should be evicted");
    assert!(store.get(&id2).is_some());
    assert!(store.get(&id3).is_some());
    assert!(store.get(&id4).is_some());

    Ok(())
}

#[test]
fn test_streaming_not_evicted() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(3));
    let now = Utc::now();
    let old = now - ChronoDuration::hours(2);

    let mut req1 = make_test_request_with_timestamp(None, "sess-1", old);
    let id1 = req1.id;
    req1.status = RequestStatus::Streaming;
    store.insert(req1);

    let req2 = make_test_request_with_timestamp(None, "sess-1", old);
    let id2 = req2.id;
    store.insert(req2);

    let req3 = make_test_request_with_timestamp(None, "sess-1", old);
    store.insert(req3);

    assert_eq!(store.len(), 3);

    let req4 = make_test_request_with_timestamp(None, "sess-1", now);
    store.insert(req4);

    assert_eq!(store.len(), 3);
    assert!(
        store.get(&id1).is_some(),
        "streaming entry should survive eviction"
    );
    assert!(
        store.get(&id2).is_none(),
        "oldest complete entry should be evicted"
    );

    Ok(())
}

#[test]
fn stream_blocks_waiting_for_the_terminal_retry_count_against_the_byte_budget() {
    let mut store = RequestStore::new(small_byte_policy(100, 0));
    let mut req = make_test_request(Some(RequestId::from_raw(624)), "s-retained");
    req.status = RequestStatus::Streaming;
    store.insert(req);
    let before = store.total_bytes();
    store.update(&RequestId::from_raw(624), |req| {
        req.failed_stream_blocks
            .push(crate::store::models::FailedStreamBlock {
                seq_ord: 0,
                block_type: "text".into(),
                content: "x".repeat(4096),
                signature: Some("sig".into()),
                wire_tool_use_id: None,
                content_sha256: "0".repeat(64),
            });
    });
    assert_eq!(store.total_bytes() - before, 4096 + 3 + 64 + 4);
    store.remove_entry(&RequestId::from_raw(624));
    assert_eq!(store.total_bytes(), 0);
}

#[test]
fn the_memory_breakdown_counts_retained_stream_blocks_as_eviction_does() {
    let mut store = RequestStore::new(small_byte_policy(100, 0));
    let mut req = make_test_request(Some(RequestId::from_raw(625)), "s-breakdown");
    req.status = RequestStatus::Streaming;
    store.insert(req);
    let (view_before, evict_before) = (store.memory_breakdown(1), store.total_bytes());
    store.update(&RequestId::from_raw(625), |req| {
        req.failed_stream_blocks
            .push(crate::store::models::FailedStreamBlock {
                seq_ord: 0,
                block_type: "text".into(),
                content: "y".repeat(1000),
                signature: None,
                wire_tool_use_id: None,
                content_sha256: "0".repeat(64),
            });
    });
    let view = store.memory_breakdown(1);
    assert_eq!(view.failed_stream_blocks_bytes, 1000 + 64 + 4);
    assert_eq!(
        view.entries_total_bytes - view_before.entries_total_bytes,
        store.total_bytes() - evict_before
    );
}
