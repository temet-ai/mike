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

fn asked(acted: f64, subject: f64) -> Asked {
    let noul = |p: f64| JevAnswer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        (ACTED.to_owned(), noul(acted)),
        (SUBJECT.to_owned(), noul(subject)),
    ]))
}

fn verdict_for(acted: f64, subject: f64, reaction: Option<&str>) -> anyhow::Result<String> {
    let (answer, _) = compose(&asked(acted, subject), reaction)
        .ok_or_else(|| anyhow::anyhow!("expected an answer"))?;
    Ok(answer.verdict)
}

#[test]
fn compose_acted_is_used_whatever_the_reaction() -> anyhow::Result<()> {
    assert_eq!(verdict_for(0.9, 0.1, None)?, "used");
    assert_eq!(verdict_for(0.9, 0.9, Some("corrected"))?, "used");
    Ok(())
}

#[test]
fn compose_not_acted_matching_subject_and_corrected_is_corrected_anyway() -> anyhow::Result<()> {
    assert_eq!(
        verdict_for(0.1, 0.8, Some("corrected"))?,
        "corrected_anyway"
    );
    Ok(())
}

#[test]
fn compose_corrected_turn_with_unrelated_memory_is_ignored() -> anyhow::Result<()> {
    assert_eq!(verdict_for(0.1, 0.2, Some("corrected"))?, "ignored");
    Ok(())
}

#[test]
fn compose_matching_subject_without_a_correction_is_ignored() -> anyhow::Result<()> {
    for reaction in [None, Some("accepted"), Some("redirected")] {
        assert_eq!(verdict_for(0.1, 0.9, reaction)?, "ignored");
    }
    Ok(())
}

#[test]
fn compose_probability_is_the_chance_of_the_chosen_verdict() {
    let (v, p) = verdict_of(0.2, 0.75, Some("corrected"));
    assert_eq!(v, "corrected_anyway");
    assert!((p - 0.6).abs() < 1e-9);
    let (v, p) = verdict_of(0.2, 0.75, None);
    assert_eq!(v, "ignored");
    assert!((p - 0.8).abs() < 1e-9);
}

#[test]
fn compose_failed_or_incomplete_answer_gives_the_model_path() {
    assert!(compose(&Asked::failed_with("timeout"), None).is_none());
    let partial = Asked::answered(BTreeMap::from([(
        ACTED.to_owned(),
        JevAnswer::Noul {
            noul: 0.9,
            confidence: None,
        },
    )]));
    assert!(compose(&partial, None).is_none());
}

#[test]
fn state_carries_the_turn_facts_the_model_prompt_carries() {
    let value = state(&item());
    assert_eq!(value["memory"]["kind"], "lesson");
    assert_eq!(value["tools_used"][0], "Bash");
    assert_eq!(value["final_reply"], "deployed through the script");
    assert_eq!(value["turn_reaction_recorded_by_the_turn_judge"], "none");
    let asked = questions();
    assert!(asked.contains_key(ACTED) && asked.contains_key(SUBJECT));
}
