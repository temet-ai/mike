use super::*;
use crate::llm::decisions::Answer;

fn purpose(topic: &str, lessons: i64) -> SkillPurpose {
    SkillPurpose {
        project_id: 1,
        topic: topic.to_owned(),
        lessons,
        shipped: false,
    }
}

fn noul_answer(p: f64) -> Answer {
    Answer::Noul {
        noul: p,
        confidence: None,
    }
}

#[test]
fn compose_picks_the_most_likely_candidate_above_the_floor() {
    let asked = Asked::answered(BTreeMap::from([
        (question_key(0), noul_answer(0.75)),
        (question_key(1), noul_answer(0.9)),
    ]));
    assert_eq!(compose(&asked, 2), Some(Some((1, 0.9))));
}

#[test]
fn compose_below_the_floor_keeps_the_topics_apart() {
    let asked = Asked::answered(BTreeMap::from([(question_key(0), noul_answer(0.4))]));
    assert_eq!(compose(&asked, 1), Some(None));
}

#[test]
fn compose_failed_answer_falls_back_to_the_word_rule() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 1), None);
}

#[test]
fn applied_for_keep_apart_answer_marks_skipped() {
    assert_eq!(applied_for(Some(None)), "skipped");
    assert_eq!(applied_for(Some(Some((0, 0.9)))), "chosen");
    assert_eq!(applied_for(None), "none");
}

#[test]
fn remembered_verdict_within_a_day_is_reused_and_expires_after() {
    let topic = purpose("remembered-worker-wake-loops", 3);
    let other = purpose("remembered-worker-message-delivery", 2);
    let key = verdict_key(&topic, &[&other]);
    let then = Instant::now();
    remember(key.clone(), None, then);
    assert_eq!(remembered(&key, then + Duration::from_secs(60)), Some(None));
    assert_eq!(remembered(&key, then + REMEMBER_FOR), None);
    remember(key.clone(), Some(&other), then);
    assert_eq!(
        remembered(&key, then),
        Some(Some("remembered-worker-message-delivery".to_owned()))
    );
}

#[test]
fn verdict_key_new_candidate_set_asks_again() {
    let topic = purpose("fleet-coordination", 3);
    let a = purpose("fleet-verification", 2);
    let b = purpose("fleet-messaging", 2);
    assert_ne!(verdict_key(&topic, &[&a]), verdict_key(&topic, &[&a, &b]));
}

#[tokio::test]
async fn plan_moves_without_a_decider_equals_the_word_rule() {
    let purposes = vec![
        purpose("deploying proxy", 4),
        purpose("proxy deploying", 2),
        purpose("database migrations", 3),
    ];
    assert_eq!(
        plan_moves(None, &purposes).await,
        plan_moves_by_words(&purposes)
    );
}

#[test]
fn questions_and_state_name_the_topic_and_each_candidate() {
    let topic = purpose("deploying proxy", 4);
    let other = purpose("proxy deploys", 2);
    let candidates = vec![&other];
    assert!(questions(&topic.topic, &candidates).contains_key("candidate_0"));
    assert_eq!(
        state(&topic, &candidates)["candidates"][0]["topic"],
        "proxy deploys"
    );
}
