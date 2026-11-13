use chrono::Duration;

use super::judge::{ASK_CAP, JudgeSectionId};
use super::{RenderedDossier, TypedView, render_inputs};
use crate::dossier::view::digest::inputs_digest;
use crate::dossier::view::full_tests::{base_time, fixture, stream_row, turn_row};
use crate::dossier::view::types::{Audience, Budget, DossierError, SessionSlices, SteererSection};

fn judge(turn_id: i64) -> Result<RenderedDossier, DossierError> {
    render_inputs(
        &fixture(),
        &Audience::Judge { turn_id },
        Budget::JudgeSlices,
    )
}

#[test]
fn the_judge_render_has_every_section_in_order_with_e_headings() -> anyhow::Result<()> {
    let rendered = judge(3)?;
    let TypedView::Judge(view) = &rendered.typed else {
        anyhow::bail!("expected a judge view");
    };
    let ids: Vec<JudgeSectionId> = view.sections.iter().map(|s| s.id).collect();
    assert_eq!(ids, JudgeSectionId::ALL.to_vec());
    assert!(rendered.text.starts_with(
        "\n\nEARLIER OPERATOR MESSAGES IN THIS SESSION (oldest first, word for word, all sent before this turn ended):\n"
    ));
    assert_eq!(
        view.sections[0].body,
        "Refactor the store. Then commit when green.\n\nand add the docs"
    );
    assert_eq!(view.sections[1].body, "Then commit when green.");
    assert_eq!(view.sections[2].body, "(none)");
    assert!(
        !rendered.text.contains("reaction"),
        "the judge sees no judge-derived reaction"
    );
    Ok(())
}

#[test]
fn the_judge_never_sees_the_subjects_own_next_message() -> anyhow::Result<()> {
    let rendered = judge(2)?;
    let TypedView::Judge(view) = &rendered.typed else {
        anyhow::bail!("expected a judge view");
    };
    assert!(!view.sections[0].body.contains("thanks, next add a retry"));
    Ok(())
}

#[test]
fn an_ask_longer_than_the_cap_is_cut_with_es_marker() -> anyhow::Result<()> {
    let mut inputs = fixture();
    let long = "x".repeat(ASK_CAP + 20);
    inputs
        .stream
        .insert(0, stream_row(5, -10, &long, "operator", None));
    let rendered = render_inputs(
        &inputs,
        &Audience::Judge { turn_id: 3 },
        Budget::JudgeSlices,
    )?;
    assert!(rendered.text.contains(" [... 20 more characters]"));
    Ok(())
}

#[test]
fn a_mismatched_budget_is_an_error() {
    let err = render_inputs(
        &fixture(),
        &Audience::Judge { turn_id: 3 },
        Budget::Chars(100),
    );
    assert!(matches!(err, Err(DossierError::BudgetMismatch { .. })));
}

#[test]
fn an_open_subject_turn_is_refused() {
    let mut inputs = fixture();
    inputs.turns[2].closed_at = None;
    let err = render_inputs(
        &inputs,
        &Audience::Judge { turn_id: 3 },
        Budget::JudgeSlices,
    );
    assert!(matches!(err, Err(DossierError::TurnNotClosedAt { .. })));
}

#[test]
fn the_session_judge_lists_every_genuine_turn_even_on_a_tight_budget() -> anyhow::Result<()> {
    let mut inputs = fixture();
    for i in 0..60 {
        let id = 100 + i;
        let text = format!("step {i}: continue the migration");
        inputs
            .stream
            .push(stream_row(id, 40 + i, &text, "operator", Some(id)));
        inputs
            .turns
            .push(turn_row(id, id, 40 + i, &text, &"done. ".repeat(400)));
    }
    let rendered = render_inputs(
        &inputs,
        &Audience::SessionJudge {
            slices: SessionSlices::default(),
        },
        Budget::Chars(4_000),
    )?;
    let turns = rendered
        .sections
        .iter()
        .find(|s| s.id == "turns")
        .ok_or_else(|| anyhow::anyhow!("no turns report"))?;
    assert_eq!(turns.items_total, 63);
    assert_eq!(
        turns.items_full + turns.items_compressed + turns.items_in_spans,
        turns.items_total
    );
    assert!(
        turns.items_in_spans > 0,
        "a tight budget folds old turns into spans"
    );
    assert!(!rendered.text.contains("reaction="));
    Ok(())
}

#[test]
fn the_steerer_render_fills_its_typed_fields() -> anyhow::Result<()> {
    let rendered = render_inputs(
        &fixture(),
        &Audience::Steerer {
            turn_id: Some(3),
            sections: vec![
                SteererSection::Ending,
                SteererSection::RecentReactions,
                SteererSection::Directives { classes: None },
                SteererSection::Asks,
            ],
        },
        Budget::Chars(24_000),
    )?;
    let TypedView::Steerer(view) = &rendered.typed else {
        anyhow::bail!("expected a steerer view");
    };
    assert_eq!(view.directives.len(), 1);
    assert_eq!(view.recent_reactions[0].operator_turn_id, 2);
    assert_eq!(
        view.recent_reactions[0].correcting_prompt,
        "thanks, next add a retry"
    );
    assert_eq!(view.rendered, rendered.text);
    assert!(rendered.text.chars().count() <= 24_000);
    Ok(())
}

#[test]
fn a_steerers_judge_reads_mid_turn_messages_without_claude_codes_wording() -> anyhow::Result<()> {
    let sentence = "This is how Claude Code surfaces messages the user sends mid-turn — within the running turn, often alongside the next tool result, rather than as a separate conversation turn. Address the message above as you continue this turn.";
    let mut inputs = fixture();
    for row in inputs.stream.iter_mut().skip(2) {
        row.text = format!(
            "The user sent a new message while you were working:\n{}\n\n{sentence}",
            row.text
        );
    }
    let rendered = render_inputs(
        &inputs,
        &Audience::Steerer {
            turn_id: None,
            sections: vec![SteererSection::RecentReactions, SteererSection::Asks],
        },
        Budget::Chars(24_000),
    )?;
    assert!(
        rendered
            .text
            .contains("The operator's reply, word for word: thanks, next add a retry\n"),
        "{}",
        rendered.text
    );
    assert!(rendered.text.contains("\n\nand add the docs\n\n"));
    assert!(!rendered.text.contains("surfaces messages"));
    assert!(!rendered.text.contains("while you were working"));
    Ok(())
}

#[test]
fn the_same_inputs_give_the_same_digest_and_bytes() -> anyhow::Result<()> {
    let a = judge(3)?;
    let b = judge(3)?;
    assert_eq!(a.inputs_digest, b.inputs_digest);
    assert_eq!(a.text, b.text);
    let mut later = fixture();
    later.as_of.at += Duration::minutes(1);
    assert_ne!(inputs_digest(&later), a.inputs_digest);
    assert_eq!(a.digest_version(), "dossier-r1/judge/slices");
    let _ = base_time();
    Ok(())
}

fn label_correction(tier: &str, split: &str) -> crate::dossier::view::types::CorrectionItem {
    use crate::dossier::view::types::{CorrectionItem, CorrectionSource};
    CorrectionItem {
        corrected_turn_id: 1,
        corrected_created_at: base_time(),
        corrected_prompt: "Refactor the store. Then commit when green.".to_owned(),
        correcting_request_id: 12,
        correcting_text: "no, you broke the tests".to_owned(),
        correcting_at: base_time() + Duration::minutes(20),
        source: CorrectionSource::Label {
            label_id: 9,
            tier: tier.to_owned(),
            split: split.to_owned(),
            created_at: base_time(),
        },
    }
}

#[test]
fn the_judge_renders_dev_gold_corrections_and_hides_holdout_and_seed() -> anyhow::Result<()> {
    let mut inputs = fixture();
    inputs.corrections = vec![
        label_correction("gold", "dev"),
        label_correction("gold", "holdout"),
        label_correction("seed", "dev"),
    ];
    let rendered = render_inputs(
        &inputs,
        &Audience::Judge { turn_id: 3 },
        Budget::JudgeSlices,
    )?;
    let TypedView::Judge(view) = &rendered.typed else {
        anyhow::bail!("expected a judge view");
    };
    assert_eq!(
        view.sections[2].body,
        "Earlier ask: Refactor the store. Then commit when green.\nThe operator's correction: no, you broke the tests"
    );
    Ok(())
}
