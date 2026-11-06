use chrono::Utc;
use mike::dossier::render::{TypedView, render};
use mike::dossier::view::types::{AsOf, Audience, Budget, Reaction, SteererSection};
use mike::dossier::view::{CorrectionSource, Needs, load};
use mike::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};

use super::seeded;
use crate::seed::database_now;
use crate::turn_facts::{Fixture, fixture};

fn verdict(
    turn_id: i64,
    mode: VerdictMode,
    bundle_id: Option<i64>,
    reaction: &'static str,
) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        turn_id,
        mode,
        bundle_id,
        rubric_version: "v2-2026-09-23",
        input_builder_version: "judge-input-v2.1",
        slices: Vec::new(),
        model: "google/gemini-3.5-flash",
        judgement: VerdictJudgement {
            shape: VerdictShape::TwoAxis {
                reaction,
                delivery: "partial",
                failure_tags: None,
                severity: None,
            },
            confidence: 0.8,
            rationale: "the operator pushed back",
            unmet_ask: "",
            evidence_quality: "asserted",
        },
        next_genuine_request_id: None,
        as_of: Utc::now(),
        input_text: "input",
        usage: VerdictCallUsage::default(),
    }
}

async fn candidate_bundle(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha)
         VALUES ('turn_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-dossier-h', 'build')
         RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

async fn steerer_reactions(
    f: &Fixture,
    third: i64,
) -> anyhow::Result<Vec<mike::dossier::render::steerer::RecentReaction>> {
    let rendered = render(
        &f.handle,
        f.session_id,
        AsOf::knowledge(database_now(f.handle.pool()).await?),
        &Audience::Steerer {
            turn_id: Some(third),
            sections: vec![SteererSection::RecentReactions],
        },
        Budget::Chars(24_000),
    )
    .await?;
    let TypedView::Steerer(view) = rendered.typed else {
        anyhow::bail!("expected the steerer view");
    };
    Ok(view.recent_reactions)
}

#[tokio::test]
async fn recent_reactions_read_latest_live_verdict_per_turn() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (first, _, third) = seeded(&f).await?;
    f.handle
        .insert_turn_verdict(verdict(first, VerdictMode::Live, None, "accepted"))
        .await?;
    let newest = f
        .handle
        .insert_turn_verdict(verdict(first, VerdictMode::Live, None, "corrected"))
        .await?;
    let reactions = steerer_reactions(&f, third).await?;
    let for_first = reactions
        .iter()
        .find(|r| r.operator_turn_id == first)
        .ok_or_else(|| anyhow::anyhow!("no reaction for the first turn: {reactions:?}"))?;
    assert_eq!(for_first.reaction, Some(Reaction::Corrected));
    assert_eq!(for_first.turn_verdict_id, Some(newest));
    let inputs = load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(database_now(f.handle.pool()).await?),
        Needs::everything(None),
    )
    .await?;
    assert!(inputs.corrections.iter().any(|c| c.corrected_turn_id == first
        && matches!(c.source, CorrectionSource::LiveVerdict { verdict_id, .. } if verdict_id == newest)));
    assert!(inputs.exclusions.iter().all(|e| e.section != "reactions"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shadow_and_eval_verdicts_never_read() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (first, second, third) = seeded(&f).await?;
    let bundle = candidate_bundle(&f).await?;
    for turn in [first, second] {
        f.handle
            .insert_turn_verdict(verdict(
                turn,
                VerdictMode::Shadow,
                Some(bundle),
                "corrected",
            ))
            .await?;
        f.handle
            .insert_turn_verdict(verdict(turn, VerdictMode::Eval, Some(bundle), "redirected"))
            .await?;
    }
    let reactions = steerer_reactions(&f, third).await?;
    assert!(
        reactions
            .iter()
            .all(|r| r.reaction.is_none() && r.turn_verdict_id.is_none())
    );
    let inputs = load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(Utc::now()),
        Needs::everything(None),
    )
    .await?;
    assert!(inputs.reactions.is_empty());
    assert!(
        inputs
            .corrections
            .iter()
            .all(|c| !matches!(c.source, CorrectionSource::LiveVerdict { .. }))
    );
    f.db.cleanup().await?;
    Ok(())
}
