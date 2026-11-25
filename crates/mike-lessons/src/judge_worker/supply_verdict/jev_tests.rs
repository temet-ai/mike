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
    match compose(&asked(acted, subject), reaction) {
        Some(Composed::Decided(answer, _)) => Ok(answer.verdict),
        other => anyhow::bail!("expected a verdict, got {other:?}"),
    }
}

fn unsure(acted: f64, subject: f64, reaction: Option<&str>) -> bool {
    compose(&asked(acted, subject), reaction) == Some(Composed::Unsure)
}

#[test]
fn compose_acted_is_used_unless_the_turn_was_corrected_on_its_subject() -> anyhow::Result<()> {
    assert_eq!(verdict_for(0.9, 0.1, None)?, "used");
    assert_eq!(verdict_for(0.9, 0.9, Some("accepted"))?, "used");
    assert_eq!(verdict_for(0.9, 0.2, Some("corrected"))?, "used");
    assert_eq!(verdict_for(0.9, 0.9, Some("corrected"))?, "came_back");
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
fn compose_unrelated_memory_is_not_applicable() -> anyhow::Result<()> {
    assert_eq!(verdict_for(0.1, 0.2, Some("corrected"))?, "not_applicable");
    assert_eq!(verdict_for(0.1, 0.49, None)?, "not_applicable");
    assert_eq!(verdict_for(0.1, 0.5, None)?, "ignored");
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
    let (v, p) = verdict_of(0.2, 0.75, Some("corrected")).unwrap_or(("none", 0.0));
    assert_eq!(v, "corrected_anyway");
    assert!((p - 0.6).abs() < 1e-9);
    let (v, p) = verdict_of(0.2, 0.75, None).unwrap_or(("none", 0.0));
    assert_eq!(v, "ignored");
    assert!((p - 0.6).abs() < 1e-9);
}

#[test]
fn compose_acted_in_the_middle_band_writes_no_verdict() {
    for acted in [0.31, 0.5, 0.69] {
        assert!(unsure(acted, 0.9, None), "{acted}");
        assert!(unsure(acted, 0.9, Some("corrected")), "{acted}");
    }
}

#[test]
fn compose_band_edges_count_at_seven_tenths_and_at_three_tenths() -> anyhow::Result<()> {
    assert_eq!(verdict_for(0.7, 0.1, None)?, "used");
    assert_eq!(verdict_for(0.3, 0.9, None)?, "ignored");
    assert_eq!(
        verdict_for(0.1, 0.7, Some("corrected"))?,
        "corrected_anyway"
    );
    assert_eq!(verdict_for(0.1, 0.3, Some("corrected"))?, "not_applicable");
    Ok(())
}

#[test]
fn compose_corrected_turn_with_a_middling_subject_writes_no_verdict() {
    assert!(unsure(0.1, 0.5, Some("corrected")));
    assert!(!unsure(0.1, 0.5, None));
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
    assert_eq!(value["memory"]["text"], "Never nohup the proxy.");
    assert!(value["memory"].get("kind").is_none());
    assert!(value["memory"].get("route").is_none());
    assert_eq!(value["tools_used"][0], "Bash");
    assert_eq!(value["final_reply"], "deployed through the script");
    assert_eq!(value["turn_reaction_recorded_by_the_turn_judge"], "none");
    let asked = questions();
    assert!(asked.contains_key(ACTED) && asked.contains_key(SUBJECT));
}

#[test]
fn memory_text_drops_the_keeper_wrapper_and_the_thread_and_entry_tags() {
    let card = "<system-reminder source=\"memory-keeper\">\n[keeper:ab12cd]\nOpen work threads this prompt may refer to:\n- alpha (implementing): alpha goal [thread:alpha]\nNext up: x [entry:12]\n</system-reminder>";
    assert_eq!(
        memory_text(card),
        "Open work threads this prompt may refer to:\n- alpha (implementing): alpha goal\nNext up: x"
    );
}

#[test]
fn memory_text_drops_the_peer_mail_envelope_and_keeps_the_body() {
    assert_eq!(
        memory_text("Mail from worker-2 (session abc): never nohup the proxy"),
        "never nohup the proxy"
    );
    assert_eq!(
        memory_text("<channel source=\"mike\" kind=\"x\">use the script</channel>"),
        "use the script"
    );
}

#[test]
fn state_sends_the_operator_fields_as_typed_and_the_memory_without_machine_tags() {
    let mut pending = item();
    pending.item_text =
        "<system-reminder source=\"memory-keeper\">\n[keeper:ab]\ncard [thread:a]\n</system-reminder>"
            .to_owned();
    let value = state(&pending);
    assert_eq!(value["memory"]["text"], "card");
    assert_eq!(value["operator_prompt"], "deploy it");
    assert_eq!(value["operator_next_message"], "thanks");
}
