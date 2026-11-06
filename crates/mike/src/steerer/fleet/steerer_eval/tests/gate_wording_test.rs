use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{make_fleet_ctx, make_verifier_steerer};
use crate::store::RequestStore;
use crate::store::retention::RetentionPolicy;

fn pattern_steerer(kind: &str, pattern: &str) -> Steerer {
    let mut steerer = make_verifier_steerer();
    steerer.kind = kind.to_string();
    steerer.invocation = "pattern".to_string();
    steerer.pattern = Some(pattern.to_string());
    steerer
}

fn response_inputs(text: &str) -> EvalInputs {
    EvalInputs {
        eval_text: text.to_string(),
        operator_ask: "finish the work".to_string(),
        last_user_message: "finish the work".to_string(),
        trigger_point: "response",
        ..Default::default()
    }
}

fn fleet_ctx() -> anyhow::Result<FleetCtx> {
    let llm = OpenRouterClient::new("sk-test".to_string(), "m".to_string(), None, None)?;
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    Ok(make_fleet_ctx(store, None, llm))
}

#[test]
fn pattern_matches_does_not_match_inside_longer_words() {
    assert!(!pattern_matches(" done", "he abandoned the plan"));
    assert!(!pattern_matches(" complete", "the work is incomplete"));
    assert!(!pattern_matches(" fixed", "the path is unfixed"));
    assert!(!pattern_matches("tests pass", "all contests passed"));
    assert!(!pattern_matches("cat", "the category list"));
}

#[test]
fn pattern_matches_finds_whole_words_and_punctuated_keywords() {
    assert!(pattern_matches(" done", "we are done with it"));
    assert!(pattern_matches("done.", "we are done."));
    assert!(pattern_matches("tests pass", "all tests pass now"));
    assert!(pattern_matches("| cat", "ls | cat"));
    assert!(pattern_matches("cat<<", "cat<<EOF"));
    assert!(pattern_matches("done", "Done"));
}

#[test]
fn verifier_gate_admits_done_claim_wording_the_row_pattern_lacks() -> anyhow::Result<()> {
    let ctx = fleet_ctx()?;
    let steerer = pattern_steerer("verifier", " done");
    for text in [
        "The feature has shipped.",
        "It landed on main this morning.",
        "All finished here.",
        "The change is ready for review.",
        "Merged and deployed.",
    ] {
        assert!(
            invocation_gate_passed(&ctx, &steerer, &response_inputs(text)),
            "the verifier gate must admit: {text}"
        );
    }
    Ok(())
}

#[test]
fn verifier_gate_declines_wording_that_only_contains_a_claim_word() -> anyhow::Result<()> {
    let ctx = fleet_ctx()?;
    let steerer = pattern_steerer("verifier", " done");
    for text in [
        "I abandoned that approach.",
        "the checklist is incomplete",
        "an already known problem",
        "the spread of results",
    ] {
        assert!(
            !invocation_gate_passed(&ctx, &steerer, &response_inputs(text)),
            "the verifier gate must decline: {text}"
        );
    }
    Ok(())
}

#[test]
fn done_claim_vocabulary_applies_only_to_the_verifier() -> anyhow::Result<()> {
    let ctx = fleet_ctx()?;
    let steerer = pattern_steerer("refocus", "unrelated keyword");
    assert!(!invocation_gate_passed(
        &ctx,
        &steerer,
        &response_inputs("The feature has shipped.")
    ));
    Ok(())
}
