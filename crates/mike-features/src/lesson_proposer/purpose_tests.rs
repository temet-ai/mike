use super::*;

fn purpose(project_id: i64, topic: &str, lessons: i64, shipped: bool) -> SkillPurpose {
    SkillPurpose {
        project_id,
        topic: topic.to_owned(),
        lessons,
        shipped,
    }
}

#[test]
fn reworded_topics_with_the_same_words_share_a_purpose() {
    assert!(same_purpose("deploying-the-proxy", "proxy-deploy-steps"));
    assert!(same_purpose("database-migrations", "database-migration"));
}

#[test]
fn identical_topics_share_a_purpose_even_when_a_single_word() {
    assert!(same_purpose("testing", "testing"));
}

#[test]
fn a_single_shared_word_is_not_the_same_purpose() {
    assert!(!same_purpose("database-migrations", "migration-rollback"));
    assert!(!same_purpose("testing", "testing-flaky-db-tests"));
}

#[test]
fn unrelated_topics_do_not_share_a_purpose() {
    assert!(!same_purpose("deploying-the-proxy", "fleet-seat-briefs"));
}

#[test]
fn an_empty_topic_shares_nothing() {
    assert!(!same_purpose("", "the-of"));
}

#[test]
fn the_shipped_topic_survives_and_the_overlapping_one_moves_onto_it() {
    let plan = plan_moves(&[
        purpose(1, "proxy-deploy-steps", 6, false),
        purpose(1, "deploying-the-proxy", 2, true),
        purpose(1, "fleet-seat-briefs", 3, false),
    ]);
    assert_eq!(
        plan.get(&1),
        Some(&vec![TopicMove {
            from: "proxy-deploy-steps".to_owned(),
            to: "deploying-the-proxy".to_owned(),
        }])
    );
}

#[test]
fn with_nothing_shipped_the_larger_topic_survives() {
    let plan = plan_moves(&[
        purpose(1, "deploying-the-proxy", 2, false),
        purpose(1, "proxy-deploy-steps", 6, false),
    ]);
    assert_eq!(
        plan.get(&1),
        Some(&vec![TopicMove {
            from: "deploying-the-proxy".to_owned(),
            to: "proxy-deploy-steps".to_owned(),
        }])
    );
}

#[test]
fn topics_in_different_projects_never_merge() {
    let plan = plan_moves(&[
        purpose(1, "deploying-the-proxy", 2, false),
        purpose(2, "proxy-deploy-steps", 6, false),
    ]);
    assert!(plan.is_empty());
}

#[test]
fn three_overlapping_topics_all_move_onto_one_survivor() {
    let plan = plan_moves(&[
        purpose(1, "deploying-the-proxy", 5, false),
        purpose(1, "proxy-deploy-steps", 3, false),
        purpose(1, "deploy-proxy", 2, false),
    ]);
    let moves = plan.get(&1).map(Vec::as_slice).unwrap_or_default();
    assert_eq!(moves.len(), 2);
    assert!(moves.iter().all(|m| m.to == "deploying-the-proxy"));
}
