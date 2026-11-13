pub mod message_tests;
pub mod recording_tests;

use super::*;
use crate::steerer::fleet::steerer_eval::TurnJudgement;

#[test]
fn apply_evidence_gate_is_a_noop_on_empty_checklist() {
    let mut verdict = SteererVerdict {
        fires: true,
        confidence: 0.9,
        reason: "drift detected".to_string(),
        nudge: "refocus".to_string(),
        need: crate::steerer::verdict::Need::High,
        checklist: Vec::new(),
    };
    apply_evidence_gate(&mut verdict);
    assert!(verdict.fires);
}

#[test]
fn apply_evidence_gate_abstains_when_no_evidenced_not_done_item() {
    use crate::steerer::verdict::ChecklistItem;

    let mut verdict = SteererVerdict {
        fires: true,
        confidence: 0.95,
        reason: "claims incomplete".to_string(),
        nudge: "check it".to_string(),
        need: crate::steerer::verdict::Need::High,
        checklist: vec![
            ChecklistItem {
                criterion: "schema migration lands".to_string(),
                status: "done".to_string(),
                evidence: "req 101".to_string(),
            },
            ChecklistItem {
                criterion: "tests pass".to_string(),
                status: "not_done".to_string(),
                evidence: String::new(),
            },
        ],
    };
    apply_evidence_gate(&mut verdict);
    assert!(
        !verdict.fires,
        "unevidenced not_done must not justify a fire"
    );
    assert_eq!(
        verdict.checklist[1].status, "not_run",
        "an unevidenced status must be overwritten to not_run"
    );
    assert_eq!(
        verdict.checklist[0].status, "done",
        "an evidenced item is left exactly as the model reported it"
    );
}

#[test]
fn apply_evidence_gate_allows_fire_when_not_done_item_has_evidence() {
    use crate::steerer::verdict::ChecklistItem;

    let mut verdict = SteererVerdict {
        fires: true,
        confidence: 0.9,
        reason: "tests never ran".to_string(),
        nudge: "run the suite".to_string(),
        need: crate::steerer::verdict::Need::High,
        checklist: vec![ChecklistItem {
            criterion: "tests pass".to_string(),
            status: "not_done".to_string(),
            evidence: "req 240734".to_string(),
        }],
    };
    apply_evidence_gate(&mut verdict);
    assert!(
        verdict.fires,
        "a genuinely evidenced not_done item must still fire"
    );
    assert_eq!(verdict.checklist[0].status, "not_done");
}

fn retention_sample_verdict(fires: bool) -> SteererVerdict {
    SteererVerdict {
        fires,
        confidence: 0.83,
        reason: "the turn shows the test output it claims".to_string(),
        nudge: String::new(),
        need: crate::steerer::verdict::Need::High,
        checklist: Vec::new(),
    }
}

fn judged(verdict: SteererVerdict, abstention: Option<Abstention>) -> TurnJudgement {
    TurnJudgement {
        verdict: Some(verdict),
        abstention,
    }
}

#[test]
fn evaluation_record_without_retention_still_records_the_judgement() -> anyhow::Result<()> {
    let judgement = judged(
        retention_sample_verdict(false),
        Some(Abstention::VerdictDeclined),
    );
    let kept = judgement
        .evaluation_record(false, false)
        .ok_or_else(|| anyhow::anyhow!("a judged pass must always leave a record"))?;
    assert_eq!(
        kept.verdict_json,
        serde_json::json!({"judged": true, "abstention": "verdict_declined"})
    );
    assert_eq!(kept.confidence, None);
    assert_eq!(
        kept.reason.as_deref(),
        Some("judged: the verdict declined to fire")
    );
    Ok(())
}

#[test]
fn evaluation_record_keeps_a_pass_and_its_abstention_when_retention_is_on() -> anyhow::Result<()> {
    let judgement = judged(
        retention_sample_verdict(false),
        Some(Abstention::VerdictDeclined),
    );
    let kept = judgement
        .evaluation_record(false, true)
        .ok_or_else(|| anyhow::anyhow!("a pass must be retained while the setting is on"))?;
    assert_eq!(kept.confidence, Some(0.83));
    assert_eq!(
        kept.reason.as_deref(),
        Some("the turn shows the test output it claims")
    );
    assert_eq!(kept.verdict_json["fires"], serde_json::json!(false));
    assert_eq!(kept.verdict_json["confidence"], serde_json::json!(0.83));
    assert_eq!(kept.verdict_json["judged"], serde_json::json!(true));
    assert_eq!(
        kept.verdict_json["abstention"],
        serde_json::json!("verdict_declined")
    );
    Ok(())
}

#[test]
fn evaluation_record_always_keeps_a_fire_without_an_abstention() -> anyhow::Result<()> {
    let judgement = judged(retention_sample_verdict(true), None);
    let kept = judgement
        .evaluation_record(true, false)
        .ok_or_else(|| anyhow::anyhow!("a fired verdict is retained regardless of the setting"))?;
    assert_eq!(kept.verdict_json["fires"], serde_json::json!(true));
    assert_eq!(kept.verdict_json["judged"], serde_json::json!(true));
    assert!(kept.verdict_json.get("abstention").is_none());
    Ok(())
}

#[test]
fn evaluation_record_records_that_nothing_was_judged() -> anyhow::Result<()> {
    let judgement = TurnJudgement::not_judged(Abstention::NoTurnClosedSteerer);
    let kept = judgement
        .evaluation_record(false, true)
        .ok_or_else(|| anyhow::anyhow!("a turn that was not judged must still be recorded"))?;
    assert_eq!(
        kept.verdict_json,
        serde_json::json!({"judged": false, "abstention": "no_turn_closed_steerer"})
    );
    assert_eq!(kept.confidence, None);
    assert_eq!(
        kept.reason.as_deref(),
        Some("not judged: no enrolled steerer is triggered at turn close")
    );
    Ok(())
}
