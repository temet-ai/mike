use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::projection::{
    DELIVERIES, OUTCOME_PROJECTION_VERSION, ProjectionMode, PromoteOutcome, REACTIONS,
    map_legacy_outcome,
};
use mike::store::persistence::turn_verdicts::VerdictMode;

use super::promotion_support::{bundle, closed_turn, gold, params, projected, set_legacy, verdict};
use crate::turn_facts::{Req, fixture, insert_request};

#[tokio::test]
async fn projection_sql_matches_rust_mapping_for_all_pairs() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let mut expected = Vec::new();
    let mut start = 20_000;
    for reaction in REACTIONS {
        for delivery in DELIVERIES {
            start += 1;
            let turn = closed_turn(&f, start).await?;
            f.handle
                .eval()
                .insert_turn_verdict(verdict(
                    turn,
                    VerdictMode::Live,
                    b,
                    (reaction, delivery),
                    0.9,
                ))
                .await?;
            expected.push((turn, reaction, delivery));
        }
    }
    let ids: Vec<i64> = expected.iter().map(|(t, _, _)| *t).collect();
    let changed = f
        .handle
        .project_turn_outcomes(&ids, &params(b, ProjectionMode::Apply))
        .await?;
    assert_eq!(changed.len(), 30);
    for (turn, reaction, delivery) in expected {
        let p = projected(&f, turn).await?;
        assert_eq!(
            p.outcome,
            map_legacy_outcome(reaction, delivery, 0.9, 0.5, false),
            "{reaction}/{delivery}"
        );
        assert_eq!(p.outcome_source.as_deref(), Some("judge"));
        assert_eq!(p.reaction.as_deref(), Some(reaction));
        assert_eq!(p.delivery.as_deref(), Some(delivery));
        assert_eq!(p.outcome_bundle_id, Some(b));
        assert_eq!(
            p.outcome_projection_version,
            Some(OUTCOME_PROJECTION_VERSION)
        );
        assert_ne!(p.outcome, "abandoned");
    }
    Ok(())
}

#[tokio::test]
async fn projection_ignores_shadow_verdicts_and_other_bundles() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let live = bundle(&f).await?;
    let other = bundle(&f).await?;
    let shadow_turn = closed_turn(&f, 21_001).await?;
    let other_turn = closed_turn(&f, 21_002).await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            shadow_turn,
            VerdictMode::Shadow,
            live,
            ("corrected", "partial"),
            0.9,
        ))
        .await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            other_turn,
            VerdictMode::Live,
            other,
            ("corrected", "partial"),
            0.9,
        ))
        .await?;
    let changed = f
        .handle
        .project_turn_outcomes(
            &[shadow_turn, other_turn],
            &params(live, ProjectionMode::Apply),
        )
        .await?;
    assert!(changed.is_empty());
    for turn in [shadow_turn, other_turn] {
        let p = projected(&f, turn).await?;
        assert_eq!(p.outcome, "unknown");
        assert_eq!(p.outcome_source, None);
    }
    Ok(())
}

#[tokio::test]
async fn projection_gold_label_wins_and_combines_axes() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 21_101).await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            turn,
            VerdictMode::Live,
            b,
            ("accepted", "shipped"),
            0.9,
        ))
        .await?;
    gold(&f, turn, "reaction", "corrected").await?;
    f.handle
        .project_turn_outcomes(&[turn], &params(b, ProjectionMode::Apply))
        .await?;
    let p = projected(&f, turn).await?;
    assert_eq!(p.outcome, "corrected");
    assert_eq!(p.outcome_source.as_deref(), Some("operator"));
    assert_eq!(p.reaction.as_deref(), Some("corrected"));
    assert_eq!(
        p.delivery.as_deref(),
        Some("shipped"),
        "the delivery axis comes from the live verdict"
    );
    assert_eq!(p.live_verdict_id, None);
    assert_eq!(p.outcome_bundle_id, None);
    Ok(())
}

#[tokio::test]
async fn projection_snapshots_legacy_label_once_and_is_idempotent() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 21_201).await?;
    set_legacy(&f, turn, "abandoned").await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            turn,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    let first = f
        .handle
        .project_turn_outcomes(&[turn], &params(b, ProjectionMode::Apply))
        .await?;
    assert_eq!(first, vec![(turn, "accepted".to_owned())]);
    let snapshot_sql =
        "SELECT row_to_json(l)::text FROM memory.operator_turn_legacy_labels l WHERE turn_id = $1";
    let before: String = sqlx::query_scalar(snapshot_sql)
        .bind(turn)
        .fetch_one(f.handle.pool())
        .await?;
    assert!(before.contains("\"rationale\":\"legacy\""), "{before}");
    let second = f
        .handle
        .project_turn_outcomes(&[turn], &params(b, ProjectionMode::Apply))
        .await?;
    assert!(second.is_empty(), "a repeat projection changes nothing");
    let after: String = sqlx::query_scalar(snapshot_sql)
        .bind(turn)
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(before, after);
    Ok(())
}

#[tokio::test]
async fn projection_never_touches_open_turns_and_resets_without_verdict() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    insert_request(&f, &Req::primary(21300, 0, "open")).await?;
    let open: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (session_id, project_id, start_request_id, end_request_id, user_prompt)
         VALUES ($1, $2, 21300, 21300, 'open') RETURNING id",
    )
    .bind(f.session_id)
    .bind(f.project_id)
    .fetch_one(f.handle.pool())
    .await?;
    let bare = closed_turn(&f, 21_301).await?;
    set_legacy(&f, bare, "corrected").await?;
    let applied = f
        .handle
        .project_turn_outcomes(&[open, bare], &params(b, ProjectionMode::Apply))
        .await?;
    assert!(
        applied.is_empty(),
        "apply mode leaves a turn without gold or verdict unchanged"
    );
    assert_eq!(projected(&f, bare).await?.outcome, "corrected");
    let reset = f
        .handle
        .project_turn_outcomes(
            &[open, bare],
            &params(b, ProjectionMode::ResetWithoutVerdict),
        )
        .await?;
    assert_eq!(reset, vec![(bare, "unknown".to_owned())]);
    let p = projected(&f, bare).await?;
    assert_eq!(p.outcome_source.as_deref(), Some("no_verdict"));
    assert_eq!(projected(&f, open).await?.outcome_source, None);
    Ok(())
}

#[tokio::test]
async fn promote_rejects_shadow_verdict_and_errs_on_a_missing_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 21_401).await?;
    let shadow = f
        .handle
        .eval()
        .insert_turn_verdict(verdict(
            turn,
            VerdictMode::Shadow,
            b,
            ("corrected", "none"),
            0.9,
        ))
        .await?;
    let outcome = f
        .handle
        .promote_turn_verdict(shadow, &params(b, ProjectionMode::Apply))
        .await?;
    assert_eq!(outcome, PromoteOutcome::NotLive);
    assert_eq!(projected(&f, turn).await?.outcome, "unknown");
    let missing = f
        .handle
        .promote_turn_verdict(i64::MAX, &params(b, ProjectionMode::Apply))
        .await;
    assert!(missing.is_err());
    Ok(())
}

#[tokio::test]
async fn live_insert_and_promote_marks_the_verdict_projected() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 21_501).await?;
    let (verdict_id, outcome) = f
        .handle
        .eval()
        .insert_live_verdict_and_promote(
            verdict(turn, VerdictMode::Live, b, ("corrected", "partial"), 0.9),
            &params(b, ProjectionMode::Apply),
        )
        .await?;
    assert!(
        matches!(outcome, PromoteOutcome::Projected { ref outcome, changed: true, .. } if outcome == "corrected")
    );
    let flagged: bool =
        sqlx::query_scalar("SELECT projected FROM memory.turn_verdicts WHERE id = $1")
            .bind(verdict_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert!(flagged);
    assert_eq!(projected(&f, turn).await?.live_verdict_id, Some(verdict_id));
    Ok(())
}

#[tokio::test]
async fn live_reactions_as_of_prefers_v2_and_falls_back_to_r3() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let v2_turn = closed_turn(&f, 21_601).await?;
    let r3_turn = closed_turn(&f, 21_602).await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            v2_turn,
            VerdictMode::Live,
            b,
            ("corrected", "partial"),
            0.9,
        ))
        .await?;
    let legacy = mike::store::persistence::turn_verdicts::TurnVerdictInsert {
        judgement: mike::store::persistence::turn_verdicts::VerdictJudgement {
            shape: mike::store::persistence::turn_verdicts::VerdictShape::Legacy {
                label: "corrected",
            },
            ..verdict(r3_turn, VerdictMode::Live, b, ("corrected", "partial"), 0.9).judgement
        },
        bundle_id: None,
        ..verdict(r3_turn, VerdictMode::Live, b, ("corrected", "partial"), 0.9)
    };
    f.handle.eval().insert_turn_verdict(legacy.clone()).await?;
    f.handle
        .eval()
        .insert_turn_verdict(mike::store::persistence::turn_verdicts::TurnVerdictInsert {
            turn_id: v2_turn,
            ..legacy
        })
        .await?;
    let as_of = crate::seed::database_now(f.handle.pool()).await?;
    let rows = f
        .handle
        .live_reactions_as_of(&[v2_turn, r3_turn], as_of)
        .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].turn_id, v2_turn);
    assert_eq!(
        rows[0].reaction.as_deref(),
        Some("corrected"),
        "an older v2 row wins over a newer r3 row"
    );
    assert_eq!(rows[1].reaction, None);
    assert_eq!(rows[1].legacy_label.as_deref(), Some("corrected"));
    Ok(())
}
