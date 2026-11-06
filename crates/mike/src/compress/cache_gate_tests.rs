use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::*;

fn body(first_prompt: &str, ttl: Option<&str>) -> (Vec<u8>, Value) {
    let mut block = json!({"type": "text", "text": "continue"});
    if let Some(ttl) = ttl {
        block["cache_control"] = json!({"type": "ephemeral", "ttl": ttl});
    }
    let value = json!({
        "model": "claude-opus-5-5",
        "messages": [
            {"role": "user", "content": first_prompt},
            {"role": "user", "content": [block]}
        ]
    });
    (serde_json::to_vec(&value).unwrap_or_default(), value)
}

const FILE_READ: &str = "file_read";

#[test]
fn cache_gate_reducers_on_one_lane_keep_separate_records() {
    let (raw, value) = body("cache_gate_two_reducers", None);
    let start = Instant::now();
    CacheGate::open_at(FILE_READ, &raw, &value, start).close_at(ids(&["toolu_read"]), start);
    let other = CacheGate::open_at(
        "shell_output",
        &raw,
        &value,
        start + Duration::from_secs(2 * 3600),
    );
    assert!(!other.is_warm());
    let same = CacheGate::open_at(FILE_READ, &raw, &value, start + Duration::from_secs(60));
    assert_eq!(same.warm_cuts(), Some(&ids(&["toolu_read"])));
}

fn ids(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn cache_gate_unseen_lane_soon_after_start_holds_new_cuts() {
    let (raw, value) = body("cache_gate_unseen_lane", None);
    let gate = CacheGate::open_at(FILE_READ, &raw, &value, *STARTED + Duration::from_secs(60));
    assert_eq!(gate.warm_cuts(), Some(&HashSet::new()));
}

#[test]
fn cache_gate_unseen_lane_an_hour_after_start_is_cold() {
    let (raw, value) = body("cache_gate_unseen_late_lane", None);
    let gate = CacheGate::open_at(
        FILE_READ,
        &raw,
        &value,
        *STARTED + Duration::from_secs(2 * 3600),
    );
    assert!(!gate.is_warm());
}

#[test]
fn cache_gate_lane_seen_within_its_ttl_is_warm_and_keeps_only_earlier_cuts() {
    let (raw, value) = body("cache_gate_warm_lane", None);
    let start = Instant::now();
    CacheGate::open_at(FILE_READ, &raw, &value, start).close_at(ids(&["toolu_a"]), start);
    let gate = CacheGate::open_at(FILE_READ, &raw, &value, start + Duration::from_secs(60));
    assert_eq!(gate.warm_cuts(), Some(&ids(&["toolu_a"])));
}

#[test]
fn cache_gate_lane_idle_past_the_five_minute_ttl_is_cold() {
    let (raw, value) = body("cache_gate_idle_lane", None);
    let start = Instant::now();
    CacheGate::open_at(FILE_READ, &raw, &value, start).close_at(ids(&["toolu_a"]), start);
    let gate = CacheGate::open_at(FILE_READ, &raw, &value, start + Duration::from_secs(290));
    assert!(!gate.is_warm());
}

#[test]
fn cache_gate_one_hour_ttl_keeps_the_lane_warm_after_five_minutes() {
    let (raw, value) = body("cache_gate_long_ttl_lane", Some("1h"));
    let start = Instant::now();
    CacheGate::open_at(FILE_READ, &raw, &value, start).close_at(HashSet::new(), start);
    let warm = CacheGate::open_at(FILE_READ, &raw, &value, start + Duration::from_secs(600));
    assert!(warm.is_warm());
    let cold = CacheGate::open_at(FILE_READ, &raw, &value, start + Duration::from_secs(3590));
    assert!(!cold.is_warm());
}

#[test]
fn cache_gate_lanes_with_different_first_messages_are_separate() {
    let (raw, value) = body("cache_gate_lane_one", None);
    let (other_raw, other) = body("cache_gate_lane_two", None);
    let start = Instant::now();
    CacheGate::open_at(FILE_READ, &raw, &value, start).close_at(ids(&["toolu_a"]), start);
    let other_gate = CacheGate::open_at(FILE_READ, &other_raw, &other, start);
    assert_eq!(other_gate.warm_cuts(), Some(&HashSet::new()));
}
