use super::*;

fn lesson(id: i64, text: &str) -> UntopicedLesson {
    UntopicedLesson {
        id,
        project_id: 7,
        text: text.to_owned(),
    }
}

#[test]
fn normalise_topic_lowercases_and_joins_words_with_single_dashes() {
    assert_eq!(
        normalise_topic("  Deploying the Proxy!! "),
        Some("deploying-the-proxy".to_owned())
    );
    assert_eq!(
        normalise_topic("db__migrations"),
        Some("db-migrations".to_owned())
    );
}

#[test]
fn normalise_topic_rejects_blank_and_kind_words() {
    assert_eq!(normalise_topic("   "), None);
    assert_eq!(normalise_topic("Warning"), None);
    assert_eq!(normalise_topic("---"), None);
}

#[test]
fn normalise_topic_caps_length_without_a_trailing_dash() {
    let long = "a-very-long-topic-name-that-keeps-going-and-going-forever";
    let topic = normalise_topic(long).unwrap_or_default();
    assert!(!topic.is_empty());
    assert!(topic.chars().count() <= 40);
    assert!(!topic.ends_with('-'));
}

#[test]
fn build_input_lists_existing_topics_then_each_lesson_by_id() {
    let input = build_input(
        &["deploying-the-proxy".to_owned()],
        &[lesson(5, "Never nohup the proxy.")],
    );
    assert!(input.starts_with("## EXISTING TOPICS\n- deploying-the-proxy\n"));
    assert!(input.contains("## LESSONS\n[5] Never nohup the proxy."));
}

#[test]
fn build_input_says_when_no_topics_exist_yet() {
    let input = build_input(&[], &[]);
    assert!(input.contains("(none yet)"));
}

#[test]
fn accepted_keeps_only_batch_ids_once_with_valid_topics() {
    let batch = vec![lesson(1, "a"), lesson(2, "b"), lesson(3, "c")];
    let answer = Assignments {
        assignments: vec![
            Assignment {
                id: 1,
                topic: "Fleet seats".to_owned(),
            },
            Assignment {
                id: 1,
                topic: "duplicate".to_owned(),
            },
            Assignment {
                id: 2,
                topic: "warning".to_owned(),
            },
            Assignment {
                id: 9,
                topic: "not-in-batch".to_owned(),
            },
            Assignment {
                id: 3,
                topic: "database migrations".to_owned(),
            },
        ],
    };
    let (ids, topics) = accepted(&batch, answer);
    assert_eq!(ids, vec![1, 3]);
    assert_eq!(topics, vec!["fleet-seats", "database-migrations"]);
}

#[test]
fn schema_requires_assignments_with_id_and_topic() {
    let s = schema();
    assert_eq!(s["required"], serde_json::json!(["assignments"]));
    assert_eq!(
        s["properties"]["assignments"]["items"]["required"],
        serde_json::json!(["id", "topic"])
    );
}
