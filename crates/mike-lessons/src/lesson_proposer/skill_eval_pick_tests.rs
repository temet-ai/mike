use super::*;
use crate::llm::decisions::Answer;

fn entries() -> Vec<(String, String)> {
    vec![
        ("deploy-proxy".to_owned(), "Deploy the proxy".to_owned()),
        ("db-migrations".to_owned(), "Write migrations".to_owned()),
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
fn compose_skill_above_the_floor_loads_it_and_none_loads_nothing() -> anyhow::Result<()> {
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("deploy-proxy", 0.9)),
        (question_key(2), pick(NONE_KEY, 0.8)),
    ]));
    let choices =
        compose(&asked, 2, &entries()).ok_or_else(|| anyhow::anyhow!("expected answers"))?;
    assert_eq!(choices[0].message, 1);
    assert_eq!(choices[0].load, vec!["deploy-proxy".to_owned()]);
    assert!(choices[1].load.is_empty());
    Ok(())
}

#[test]
fn compose_low_probability_or_unknown_skill_loads_nothing() -> anyhow::Result<()> {
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), pick("deploy-proxy", 0.3)),
        (question_key(2), pick("invented", 0.9)),
    ]));
    let choices =
        compose(&asked, 2, &entries()).ok_or_else(|| anyhow::anyhow!("expected answers"))?;
    assert!(choices.iter().all(|c| c.load.is_empty()));
    Ok(())
}

#[test]
fn compose_failed_answer_gives_the_model_path() {
    assert!(compose(&Asked::failed_with("timeout"), 2, &entries()).is_none());
}

#[test]
fn questions_ask_one_choice_per_message_over_the_skills_and_none() -> anyhow::Result<()> {
    let asked = questions(&entries(), 2);
    assert_eq!(asked.len(), 2);
    let Some(Question::Choice { criteria, .. }) = asked.get("message_2") else {
        anyhow::bail!("expected a choice question");
    };
    assert!(criteria.contains_key(NONE_KEY));
    assert!(criteria.contains_key("db-migrations"));
    assert_eq!(
        state("a project", &["first", "second"])["messages"][1]["number"],
        2
    );
    Ok(())
}
