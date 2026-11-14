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
