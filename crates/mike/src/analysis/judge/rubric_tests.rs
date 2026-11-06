use sha2::{Digest, Sha256};

use super::rubric::{R3, R3_ID, RubricFamily, V2_2026_09_23, effective_rubric, latest, rubric};

fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn r3_text_is_byte_equal_to_the_live_prompt() {
    assert_eq!(
        hex(R3.system_prompt),
        "cd74fb4b4b45a980083d461f40f4e968211741c1b8042f5fef46509ebe6dadc9"
    );
    assert_eq!(super::judge_system_prompt(), R3.system_prompt);
}

#[test]
fn v2_text_is_byte_equal_to_the_measured_candidate() {
    assert_eq!(V2_2026_09_23.system_prompt.len(), 3_982);
    assert_eq!(
        hex(V2_2026_09_23.system_prompt),
        "676df07afa2e8a3286dae8fa91344d16cb8ea699997011b54f7cd2db6838b398"
    );
}

#[test]
fn the_effective_id_carries_a_hash_only_for_a_differing_override() {
    assert_eq!(effective_rubric(&R3, None).id, R3_ID);
    assert_eq!(effective_rubric(&R3, Some("  ")).id, R3_ID);
    assert_eq!(effective_rubric(&R3, Some(R3.system_prompt)).id, R3_ID);
    let edited = effective_rubric(&R3, Some("judge harder"));
    assert!(edited.id.starts_with("r3-2026-08-29+"));
    assert_eq!(edited.id.len(), R3_ID.len() + 13);
    assert_eq!(edited.system_prompt, "judge harder");
}

#[test]
fn the_registry_finds_rubrics_by_id_and_family() {
    assert_eq!(
        rubric("v2-2026-09-23").map(|r| r.family),
        Some(RubricFamily::TwoAxis)
    );
    assert!(rubric("nope").is_none());
    assert_eq!(latest(RubricFamily::LegacyOutcome).id, R3_ID);
}
