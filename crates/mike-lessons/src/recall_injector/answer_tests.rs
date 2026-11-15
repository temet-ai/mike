use super::*;

fn row(id: i64, topic: Option<&str>) -> LessonRow {
    LessonRow {
        id,
        project_id: None,
        project_path: None,
        kind: "rule".to_owned(),
        text: "text".to_owned(),
        agent_id: None,
        stage: "live".to_owned(),
        source_turn_id: None,
        created_at: chrono::Utc::now(),
        expires_at: None,
        topic: topic.map(str::to_owned),
    }
}

fn topics(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

#[test]
fn unmatched_typed_topics_without_a_row_are_returned_in_order() {
    let rows = vec![row(1, Some("deploy")), row(2, None)];
    let missing = unmatched(&topics(&["zzz", "deploy", "aaa"]), &rows);
    assert_eq!(missing, topics(&["zzz", "aaa"]));
}

#[test]
fn unmatched_with_no_rows_returns_every_typed_topic() {
    let missing = unmatched(&topics(&["deploy", "zzz"]), &[]);
    assert_eq!(missing, topics(&["deploy", "zzz"]));
}

#[test]
fn read_topics_with_a_remembered_mapping_include_the_mapped_topic() {
    let project = format!("/tmp/answer-recall-{}", Uuid::new_v4());
    topic_memory::remember(&project, "deploy", "deploying-the-proxy");
    let read = topic_memory::expand(&project, &topics(&["deploy", "other"]));
    assert_eq!(read, topics(&["deploy", "other", "deploying-the-proxy"]));
}

#[test]
fn read_topics_without_a_remembered_mapping_are_the_typed_topics() {
    let project = format!("/tmp/answer-recall-{}", Uuid::new_v4());
    let read = topic_memory::expand(&project, &topics(&["deploy"]));
    assert_eq!(read, topics(&["deploy"]));
}

#[test]
fn unasked_topics_after_mark_asked_leave_out_the_marked_ones() {
    let project = format!("/tmp/answer-recall-{}", Uuid::new_v4());
    topic_memory::mark_asked(&project, &topics(&["zzz"]));
    let left = topic_memory::unasked(&project, &topics(&["zzz", "new"]));
    assert_eq!(left, topics(&["new"]));
}
