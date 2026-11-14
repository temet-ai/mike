use super::*;
use crate::llm::decisions::Answer;

fn lesson(id: i64) -> UntopicedLesson {
    UntopicedLesson {
        id,
        project_id: 7,
        text: format!("lesson text {id}"),
    }
}

fn existing() -> Vec<String> {
    vec![
        "deploying-the-proxy".to_owned(),
        "database-migrations".to_owned(),
    ]
}

fn pick(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

#[test]
fn compose_existing_topic_above_the_floor_is_assigned_and_new_is_left_for_the_model() {
    let lessons = [lesson(1), lesson(2)];
    let group: Vec<&UntopicedLesson> = lessons.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("database-migrations", 0.9)),
        (question_key(2), pick(NEW_KEY, 0.8)),
    ]));
    assert_eq!(
        compose(&asked, &group, &existing()),
        vec![(1, "database-migrations".to_owned(), 0.9)]
    );
}

#[test]
fn compose_unknown_topic_or_low_probability_assigns_nothing() {
    let lessons = [lesson(1), lesson(2)];
    let group: Vec<&UntopicedLesson> = lessons.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("invented", 0.95)),
        (question_key(2), pick("database-migrations", 0.3)),
    ]));
    assert!(compose(&asked, &group, &existing()).is_empty());
}

#[test]
fn compose_failed_answer_assigns_nothing() {
    let lessons = [lesson(1)];
    let group: Vec<&UntopicedLesson> = lessons.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &group, &existing()).is_empty());
}

#[tokio::test]
async fn assign_to_existing_without_a_decider_assigns_nothing() {
    let assigned = assign_to_existing(None, &existing(), &[lesson(1)]).await;
    assert!(assigned.ids.is_empty());
    assert!(assigned.topics.is_empty());
}

#[test]
fn questions_offer_the_existing_topics_and_new_for_each_lesson() -> anyhow::Result<()> {
    let lessons = [lesson(3)];
    let group: Vec<&UntopicedLesson> = lessons.iter().collect();
    let asked = questions(&existing(), &group);
    let Some(Question::Choice { criteria, .. }) = asked.get("lesson_3") else {
        anyhow::bail!("expected a choice question");
    };
    assert!(criteria.contains_key(NEW_KEY));
    assert!(criteria.contains_key("deploying-the-proxy"));
    assert_eq!(state(&existing(), &group)["lessons"][0]["id"], 3);
    Ok(())
}
