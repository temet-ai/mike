use std::time::Instant;

use super::*;

fn topics(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

#[test]
fn get_remembered_mapping_inside_a_day_returns_the_mapped_topic() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    let later = now + MAPPING_LIFETIME - Duration::from_secs(1);
    assert_eq!(
        memory.get("/p", "deploy", later),
        Some(Known::Mapped("deploying".to_owned()))
    );
}

#[test]
fn get_remembered_mapping_after_a_day_returns_nothing() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    assert_eq!(memory.get("/p", "deploy", now + MAPPING_LIFETIME), None);
}

#[test]
fn get_unmapped_answer_expires_before_a_mapping_would() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "zzz", Known::Unmapped, now);
    assert_eq!(
        memory.get("/p", "zzz", now + Duration::from_secs(60)),
        Some(Known::Unmapped)
    );
    assert_eq!(memory.get("/p", "zzz", now + UNMAPPED_LIFETIME), None);
}

#[test]
fn get_same_topic_in_another_project_returns_nothing() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    assert_eq!(memory.get("/q", "deploy", now), None);
}

#[test]
fn put_past_capacity_drops_the_oldest_entry() {
    let memory = TopicMemory::default();
    let start = Instant::now();
    for index in 0..CAPACITY {
        let at = start + Duration::from_millis(u64::try_from(index).unwrap_or(0));
        memory.put("/p", &format!("t{index}"), Known::Unmapped, at);
    }
    let now = start + Duration::from_secs(10);
    memory.put("/p", "fresh", Known::Mapped("m".to_owned()), now);
    assert_eq!(memory.len(), CAPACITY);
    assert_eq!(memory.get("/p", "t0", now), None);
    assert!(memory.get("/p", "t1", now).is_some());
    assert!(memory.get("/p", "fresh", now).is_some());
}

#[test]
fn put_past_capacity_prefers_dropping_expired_entries() {
    let memory = TopicMemory::default();
    let start = Instant::now();
    for index in 0..CAPACITY {
        memory.put("/p", &format!("t{index}"), Known::Unmapped, start);
    }
    let now = start + UNMAPPED_LIFETIME + Duration::from_secs(1);
    memory.put("/p", "fresh", Known::Unmapped, now);
    assert_eq!(memory.len(), 1);
}

#[test]
fn put_existing_key_at_capacity_replaces_without_dropping_another() {
    let memory = TopicMemory::default();
    let start = Instant::now();
    for index in 0..CAPACITY {
        memory.put("/p", &format!("t{index}"), Known::Unmapped, start);
    }
    memory.put("/p", "t0", Known::Mapped("m".to_owned()), start);
    assert_eq!(memory.len(), CAPACITY);
    assert_eq!(
        memory.get("/p", "t0", start),
        Some(Known::Mapped("m".to_owned()))
    );
}

#[test]
fn expand_typed_topic_with_a_mapping_adds_the_mapped_topic_after_the_typed_one() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    memory.put("/p", "zzz", Known::Unmapped, now);
    let read = memory.expand("/p", &topics(&["deploy", "zzz", "other"]), now);
    assert_eq!(read, topics(&["deploy", "zzz", "other", "deploying"]));
}

#[test]
fn expand_mapped_topic_already_typed_is_not_repeated() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    let read = memory.expand("/p", &topics(&["deploy", "deploying"]), now);
    assert_eq!(read, topics(&["deploy", "deploying"]));
}

#[test]
fn unasked_leaves_out_topics_with_a_live_answer_of_either_kind() {
    let memory = TopicMemory::default();
    let now = Instant::now();
    memory.put("/p", "deploy", Known::Mapped("deploying".to_owned()), now);
    memory.put("/p", "zzz", Known::Unmapped, now);
    let asked = memory.unasked("/p", &topics(&["deploy", "zzz", "new"]), now);
    assert_eq!(asked, topics(&["new"]));
}
