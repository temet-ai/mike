use super::*;
use crate::llm::decisions::Answer;

fn catalog() -> Vec<String> {
    vec![
        "deploying-the-proxy".to_owned(),
        "database-migrations".to_owned(),
    ]
}

fn missing() -> Vec<String> {
    vec!["deploy".to_owned(), "zzz".to_owned()]
}

fn choice_answer(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

#[test]
fn compose_catalog_topic_above_the_floor_maps_the_typed_topic() {
    let asked = Asked::answered(BTreeMap::from([
        (
            "typed_0".to_owned(),
            choice_answer("deploying-the-proxy", 0.9),
        ),
        ("typed_1".to_owned(), choice_answer(NONE_KEY, 0.8)),
    ]));
    let mapped = compose(&asked, &missing(), &catalog());
    assert_eq!(
        mapped,
        vec![Mapping {
            typed: "deploy".to_owned(),
            mapped: "deploying-the-proxy".to_owned(),
            probability: 0.9,
        }]
    );
}

#[test]
fn compose_low_probability_or_unknown_topic_maps_nothing() {
    let asked = Asked::answered(BTreeMap::from([
        (
            "typed_0".to_owned(),
            choice_answer("deploying-the-proxy", 0.4),
        ),
        ("typed_1".to_owned(), choice_answer("invented-topic", 0.95)),
    ]));
    assert!(compose(&asked, &missing(), &catalog()).is_empty());
}

#[test]
fn compose_failed_answer_maps_nothing() {
    assert!(compose(&Asked::failed_with("timeout"), &missing(), &catalog()).is_empty());
}

#[test]
fn questions_ask_one_choice_per_typed_topic_over_the_catalog_and_none() -> anyhow::Result<()> {
    let asked = questions(&missing(), &catalog());
    assert_eq!(asked.len(), 2);
    let Some(Question::Choice {
        criteria,
        instructions,
    }) = asked.get("typed_0")
    else {
        anyhow::bail!("expected a choice question");
    };
    assert!(instructions.contains("\"deploy\""));
    assert!(criteria.contains_key(NONE_KEY));
    assert!(criteria.contains_key("database-migrations"));
    Ok(())
}

#[test]
fn state_lists_the_typed_topics_and_the_catalog() {
    let value = state(&missing(), &catalog(), "text");
    assert_eq!(value["typed_topics"][0], "deploy");
    assert_eq!(value["catalog_topics"][1], "database-migrations");
    assert_eq!(value["session_text_before_the_request"], "text");
}
