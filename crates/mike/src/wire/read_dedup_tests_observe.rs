use super::tests_support::ts;
use super::*;
use serde_json::json;

#[test]
fn no_op_on_plain_user_prompt() {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    });
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    assert!(observe(&body, &mut index, &mut stats, ts()).is_empty());
    assert!(index.is_empty());
    assert_eq!(stats.hit_count, 0);
}

#[test]
fn first_read_indexes_but_is_not_a_hit() {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    let hits = observe(&body, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert_eq!(stats.hit_count, 0);
    assert_eq!(index.len(), 1);
    assert!(stats.read_bytes_total > 0);
}

#[test]
fn duplicate_read_same_path_same_content_is_a_hit() -> anyhow::Result<()> {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let first = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    assert!(observe(&first, &mut index, &mut stats, ts()).is_empty());

    let second = json!({
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
    let hits = observe(&second, &mut index, &mut stats, ts());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file_path, "/tmp/a.rs");
    assert_eq!(hits[0].prior_tool_use_id, "t1");
    assert_eq!(stats.hit_count, 1);
    assert!(stats.would_save_bytes > 0);
    let path_stats = stats
        .per_path
        .get("/tmp/a.rs")
        .ok_or_else(|| anyhow::anyhow!("path tracked"))?;
    assert_eq!(path_stats.hit_count, 1);
    Ok(())
}

#[test]
fn different_content_same_path_is_not_a_hit() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let first = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    observe(&first, &mut index, &mut stats, ts());

    let second = json!({
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
                {"type": "tool_result", "tool_use_id": "t2", "content": "fn main() { changed(); }"}
            ]}
        ]
    });
    let hits = observe(&second, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert_eq!(stats.hit_count, 0);
}

#[test]
fn edit_invalidates_so_next_identical_read_is_not_a_hit() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let read1 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    observe(&read1, &mut index, &mut stats, ts());
    assert_eq!(index.len(), 1);

    let edit = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Edit", "input": {
                    "file_path": "/tmp/a.rs", "old_string": "a", "new_string": "b"
                }}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "edited"}
            ]}
        ]
    });
    observe(&edit, &mut index, &mut stats, ts());
    assert!(index.is_empty(), "Edit on the indexed path must evict it");

    let read2 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t3", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t3", "content": "fn main() {}"}
            ]}
        ]
    });
    let hits = observe(&read2, &mut index, &mut stats, ts());
    assert!(
        hits.is_empty(),
        "post-Edit re-read of the same content must NOT be counted as a hit"
    );
    assert_eq!(stats.hit_count, 0);
}

#[test]
fn cat_bash_dump_is_read_like_and_hashes_content() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let first = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cat /tmp/dump.yml"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "key: value"}
            ]}
        ]
    });
    observe(&first, &mut index, &mut stats, ts());
    assert_eq!(index.len(), 1);

    let second = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cat /tmp/dump.yml"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "key: value"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "cat -n /tmp/dump.yml"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "1\tkey: value"}
            ]}
        ]
    });
    let hits = observe(&second, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert_eq!(
        index.len(),
        2,
        "distinct content hashes to a distinct entry"
    );
}

#[test]
fn bash_command_containing_path_invalidates_it() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();

    let read1 = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]}
        ]
    });
    observe(&read1, &mut index, &mut stats, ts());
    assert_eq!(index.len(), 1);

    let mv = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "mv /tmp/a.rs /tmp/b.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "moved"}
            ]}
        ]
    });
    observe(&mv, &mut index, &mut stats, ts());
    assert!(
        index.is_empty(),
        "mv referencing the indexed path must evict it"
    );
}

#[test]
fn tail_role_system_is_ignored() {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}"}
            ]},
            {"role": "system", "content": "mid-conversation note"}
        ]
    });
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    let hits = observe(&body, &mut index, &mut stats, ts());
    assert!(hits.is_empty());
    assert_eq!(
        index.len(),
        1,
        "trailing system entry must not hide the tool-loop tail"
    );
}

#[test]
fn stub_text_matches_the_design_doc_format() -> anyhow::Result<()> {
    let hit = DedupHit {
        file_path: "/tmp/a.rs".to_string(),
        prior_tool_use_id: "toolu_01abc".to_string(),
        prior_timestamp: DateTime::parse_from_rfc3339("2026-08-16T12:00:00Z")?.with_timezone(&Utc),
        byte_len: 42,
    };
    assert_eq!(
        stub_text(&hit),
        "[proxy: duplicate read suppressed — /tmp/a.rs unchanged since tool_use_id \
         toolu_01abc at 2026-08-16T12:00:00+00:00, which already returned this content in \
         this session. Refer to that turn's result. If you need to re-verify, re-issue with \
         force=true or after editing the file.]"
    );
    Ok(())
}

#[test]
fn index_stays_bounded_under_a_long_synthetic_session() {
    let mut index = SessionDedupIndex::default();
    let mut stats = SessionDedupStats::default();
    let n = MAX_DEDUP_INDEX_ENTRIES_PER_SESSION + 500;
    for i in 0..n {
        let body = json!({
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": format!("t{i}"), "name": "Read", "input": {"file_path": format!("/tmp/file-{i}.rs")}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": format!("t{i}"), "content": format!("content of file {i}")}
                ]}
            ]
        });
        observe(&body, &mut index, &mut stats, ts());
    }
    assert!(
        index.len() <= MAX_DEDUP_INDEX_ENTRIES_PER_SESSION,
        "index must never exceed the cap, got {}",
        index.len()
    );
    assert_eq!(
        index.len(),
        MAX_DEDUP_INDEX_ENTRIES_PER_SESSION,
        "eviction must have engaged to hold the index at the cap after {n} distinct reads"
    );
}

#[test]
fn enforce_cap_evicts_the_least_recently_touched_entry() -> anyhow::Result<()> {
    let mut index = SessionDedupIndex::default();
    let base = DateTime::parse_from_rfc3339("2026-08-16T00:00:00Z")?.with_timezone(&Utc);
    for i in 0..MAX_DEDUP_INDEX_ENTRIES_PER_SESSION {
        index.insert(
            (format!("/tmp/file-{i}.rs"), format!("sha-{i}")),
            DedupIndexEntry {
                tool_use_id: format!("t{i}"),
                timestamp: base + chrono::Duration::seconds(i as i64),
                byte_len: 10,
            },
        );
    }
    assert_eq!(index.len(), MAX_DEDUP_INDEX_ENTRIES_PER_SESSION);

    let newest_key = ("/tmp/newest.rs".to_string(), "sha-newest".to_string());
    index.insert(
        newest_key.clone(),
        DedupIndexEntry {
            tool_use_id: "t-newest".to_string(),
            timestamp: base + chrono::Duration::seconds(MAX_DEDUP_INDEX_ENTRIES_PER_SESSION as i64),
            byte_len: 10,
        },
    );
    assert_eq!(index.len(), MAX_DEDUP_INDEX_ENTRIES_PER_SESSION + 1);

    enforce_cap(&mut index);

    assert_eq!(index.len(), MAX_DEDUP_INDEX_ENTRIES_PER_SESSION);
    assert!(
        !index.contains_key(&("/tmp/file-0.rs".to_string(), "sha-0".to_string())),
        "the entry with the minimum timestamp must be evicted"
    );
    assert!(
        index.contains_key(&newest_key),
        "the most-recently-touched entry must survive"
    );
    Ok(())
}
