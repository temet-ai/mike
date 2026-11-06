use super::*;

#[test]
fn dedup_stats_zeroed_for_unknown_session() {
    let store = RequestStore::new(small_policy(10));
    let stats = store.dedup_stats("no-such-session");
    assert_eq!(stats.hit_count, 0);
    assert_eq!(stats.would_save_bytes, 0);
    assert!(stats.per_path.is_empty());
}

#[test]
fn observe_read_dedup_records_hit_on_duplicate_read() {
    let mut store = RequestStore::new(small_policy(10));
    let now: DateTime<Utc> = Utc::now();

    let first = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    assert!(store.observe_read_dedup("sess-dd", &first, now).is_empty());

    let second = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
            ]}
        ]
    });
    assert_eq!(store.observe_read_dedup("sess-dd", &second, now).len(), 1);

    let stats = store.dedup_stats("sess-dd");
    assert_eq!(stats.hit_count, 1);
    assert!(stats.would_save_bytes > 0);

    let other = store.dedup_stats("sess-other");
    assert_eq!(other.hit_count, 0);
}

#[test]
fn dedup_index_survives_request_eviction() {
    let mut store = RequestStore::new(small_policy(10));
    let now: DateTime<Utc> = Utc::now();
    let req = make_test_request(None, "sess-dd-evict");
    let id = req.id;
    store.insert(req);

    let first = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    store.observe_read_dedup("sess-dd-evict", &first, now);

    store.remove_entry(&id);

    let second = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
            ]}
        ]
    });
    assert_eq!(
        store
            .observe_read_dedup("sess-dd-evict", &second, now)
            .len(),
        1,
        "dedup index must survive per-request eviction"
    );
}

#[test]
fn rewrite_read_dedup_records_hit_and_returns_rewritten_body() {
    let mut store = RequestStore::new(small_policy(10));
    let now: DateTime<Utc> = Utc::now();

    let first = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    let (hits, rewritten) = store.rewrite_read_dedup("sess-dd-rw", &first, now);
    assert!(hits.is_empty());
    assert!(
        rewritten.is_none(),
        "a first-time Read has no prior hit to rewrite"
    );

    let second = serde_json::json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() {}"}
            ]}
        ]
    });
    let (hits, rewritten) = store.rewrite_read_dedup("sess-dd-rw", &second, now);
    assert_eq!(hits.len(), 1);
    assert!(
        rewritten.is_some(),
        "a duplicate-content hit must be rewritten"
    );

    let stats = store.dedup_stats("sess-dd-rw");
    assert_eq!(
        stats.hit_count, 1,
        "measurement bookkeeping runs the same whether armed or not"
    );
}
