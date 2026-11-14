use super::*;
use crate::llm::decisions::Answer as JevAnswer;

fn item() -> PendingVerdict {
    PendingVerdict {
        item_id: 1,
        item_kind: "lesson".to_owned(),
        route: "injected".to_owned(),
        item_text: "Never nohup the proxy.".to_owned(),
        session_id: 5,
        turn_id: 9,
        user_prompt: Some("deploy it".to_owned()),
        final_assistant_text: Some("deployed through the script".to_owned()),
        tools_used: vec!["Bash".to_owned()],
        files_touched: vec!["scripts/start-proxy.sh".to_owned()],
        reaction: None,
        next_message: Some("thanks".to_owned()),
    }
}

fn verdict(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(
        QUESTION.to_owned(),
        JevAnswer::Choice {
            choice: key.to_owned(),
            probabilities: BTreeMap::from([(key.to_owned(), p)]),
            confidence: None,
        },
    )]))
}

#[test]
fn compose_each_verdict_becomes_an_answer_with_its_probability() -> anyhow::Result<()> {
    for key in ["used", "corrected_anyway", "ignored"] {
        let (answer, p) =
            compose(&verdict(key, 0.8)).ok_or_else(|| anyhow::anyhow!("expected an answer"))?;
        assert_eq!(answer.verdict, key);
        assert!((p - 0.8).abs() < 1e-9);
    }
    Ok(())
}

#[test]
fn compose_unknown_verdict_or_failed_answer_gives_the_model_path() {
    assert!(compose(&verdict("maybe", 0.9)).is_none());
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn state_carries_the_turn_facts_the_model_prompt_carries() {
    let value = state(&item());
    assert_eq!(value["memory"]["kind"], "lesson");
    assert_eq!(value["tools_used"][0], "Bash");
    assert_eq!(value["final_reply"], "deployed through the script");
    assert_eq!(value["turn_reaction_recorded_by_the_turn_judge"], "none");
    assert!(questions().contains_key("verdict"));
}
