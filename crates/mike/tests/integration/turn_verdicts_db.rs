use chrono::{Duration, Utc};
use mike::store::persistence::PersistError;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::turn_verdicts::{
    LegacyStamp, TurnVerdictFilter, TurnVerdictInsert, VerdictCallUsage, VerdictJudgement,
    VerdictMode, VerdictShape,
};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn, sample_row};

fn legacy(turn_id: i64, label: &'static str) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        turn_id,
        mode: VerdictMode::Live,
        bundle_id: None,
        rubric_version: "r3-2026-08-29",
        input_builder_version: "judge-input-r3",
        slices: Vec::new(),
        model: "google/gemini-3.5-flash",
        judgement: VerdictJudgement {
            shape: VerdictShape::Legacy { label },
            confidence: 0.9,
            rationale: "the operator moved on",
            unmet_ask: "",
            evidence_quality: "asserted",
        },
        next_genuine_request_id: None,
        as_of: Utc::now(),
        input_text: "OPERATOR REQUEST",
        usage: VerdictCallUsage::default(),
    }
}

fn two_axis(turn_id: i64, severity: Option<&'static str>) -> TurnVerdictInsert<'static> {
    TurnVerdictInsert {
        judgement: VerdictJudgement {
            shape: VerdictShape::TwoAxis {
                reaction: "corrected",
                delivery: "partial",
                failure_tags: Some(vec!["missed_ask".to_owned()]),
                severity,
            },
            ..legacy(turn_id, "unknown").judgement
        },
        rubric_version: "v2-2026-09-23",
        input_builder_version: "judge-input-v2.1",
        mode: VerdictMode::Live,
        ..legacy(turn_id, "unknown")
    }
}

async fn closed_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "do the thing")).await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

async fn outcome_code(f: &Fixture, turn_id: i64) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT o.code FROM memory.operator_turns t JOIN memory.turn_outcomes o ON o.id = t.outcome_id WHERE t.id = $1",
    )
    .bind(turn_id)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn insert_turn_verdict_fills_session_id_from_the_turn() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 101).await?;
    let id = f
        .handle
        .eval()
        .insert_turn_verdict(legacy(turn, "accepted"))
        .await?;
    let session_id: i64 =
        sqlx::query_scalar("SELECT session_id FROM memory.turn_verdicts WHERE id = $1")
            .bind(id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(session_id, f.session_id);
    Ok(())
}

#[tokio::test]
async fn insert_turn_verdict_for_missing_turn_returns_not_found() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let result = f
        .handle
        .eval()
        .insert_turn_verdict(legacy(987_654_321, "accepted"))
        .await;
    assert!(matches!(
        result,
        Err(PersistError::TurnNotFound(987_654_321))
    ));
    Ok(())
}

#[tokio::test]
async fn a_two_axis_verdict_with_tags_and_major_severity_inserts_and_huge_is_rejected()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 111).await?;
    f.handle
        .eval()
        .insert_turn_verdict(two_axis(turn, Some("major")))
        .await?;
    let rejected = f
        .handle
        .eval()
        .insert_turn_verdict(two_axis(turn, Some("huge")))
        .await;
    let message = rejected.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        message.contains("turn_verdicts_severity_check"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn insert_turn_verdict_rejects_projected_shadow_and_bundleless_shadow() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 121).await?;
    let shadow = TurnVerdictInsert {
        mode: VerdictMode::Shadow,
        ..two_axis(turn, None)
    };
    let message = f
        .handle
        .eval()
        .insert_turn_verdict(shadow)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(message.contains("turn_verdicts_bundle_check"), "{message}");
    Ok(())
}

#[tokio::test]
async fn live_legacy_insert_and_stamp_projects_when_turn_is_unknown() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 131).await?;
    let (_, projected) = f
        .handle
        .eval()
        .insert_live_legacy_verdict_and_stamp(
            legacy(turn, "shipped"),
            LegacyStamp {
                outcome_code: "shipped",
            },
        )
        .await?;
    assert!(projected);
    assert_eq!(outcome_code(&f, turn).await?, "shipped");
    let (as_of, closed_at): (chrono::DateTime<Utc>, chrono::DateTime<Utc>) = sqlx::query_as(
        "SELECT v.as_of, t.closed_at FROM memory.turn_verdicts v JOIN memory.operator_turns t ON t.id = v.operator_turn_id WHERE t.id = $1",
    )
    .bind(turn)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(as_of, closed_at);
    Ok(())
}

#[tokio::test]
async fn live_legacy_insert_and_stamp_keeps_verdict_when_turn_already_labelled()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 141).await?;
    f.handle
        .eval()
        .insert_live_legacy_verdict_and_stamp(
            legacy(turn, "shipped"),
            LegacyStamp {
                outcome_code: "shipped",
            },
        )
        .await?;
    let (_, projected) = f
        .handle
        .eval()
        .insert_live_legacy_verdict_and_stamp(
            legacy(turn, "corrected"),
            LegacyStamp {
                outcome_code: "corrected",
            },
        )
        .await?;
    assert!(!projected);
    assert_eq!(outcome_code(&f, turn).await?, "shipped");
    let (rows, total) = f
        .handle
        .eval()
        .list_turn_verdicts_for_turn(turn, None, 50, 0)
        .await?;
    assert_eq!(total, 2);
    assert_eq!(rows[0].legacy_label.as_deref(), Some("corrected"));
    Ok(())
}

#[tokio::test]
async fn list_turn_verdicts_for_turn_pages_whole_rows_newest_first_with_total() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 151).await?;
    let long = "x".repeat(20_000);
    for _ in 0..3 {
        let v = TurnVerdictInsert {
            input_text: &long,
            ..legacy(turn, "accepted")
        };
        f.handle.eval().insert_turn_verdict(v).await?;
    }
    let (page, total) = f
        .handle
        .eval()
        .list_turn_verdicts_for_turn(turn, Some(VerdictMode::Live), 2, 1)
        .await?;
    assert_eq!(total, 3);
    assert_eq!(page.len(), 2);
    assert!(page[0].id > page[1].id);
    assert_eq!(page[0].input_text, long);
    let (none, zero) = f
        .handle
        .eval()
        .list_turn_verdicts_for_turn(turn, Some(VerdictMode::Eval), 50, 0)
        .await?;
    assert!(none.is_empty());
    assert_eq!(zero, 0);
    Ok(())
}

#[tokio::test]
async fn list_turn_verdicts_filters_by_mode_rubric_and_window_and_counts_total()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 161).await?;
    f.handle
        .eval()
        .insert_turn_verdict(legacy(turn, "accepted"))
        .await?;
    f.handle
        .eval()
        .insert_turn_verdict(two_axis(turn, None))
        .await?;
    let filter = TurnVerdictFilter {
        mode: Some(VerdictMode::Live),
        rubric_version: Some("v2-2026-09-23".to_owned()),
        since: Some(Utc::now() - Duration::hours(1)),
        until: Some(Utc::now() + Duration::hours(1)),
        limit: 50,
        ..TurnVerdictFilter::default()
    };
    let (rows, total) = f.handle.eval().list_turn_verdicts(&filter).await?;
    assert_eq!(total, 1);
    assert_eq!(rows[0].reaction.as_deref(), Some("corrected"));
    let future = TurnVerdictFilter {
        since: Some(Utc::now() + Duration::hours(1)),
        limit: 50,
        ..TurnVerdictFilter::default()
    };
    assert_eq!(f.handle.eval().list_turn_verdicts(&future).await?.1, 0);
    Ok(())
}

#[tokio::test]
async fn fetch_judge_turn_contexts_reads_next_genuine_text_from_turn_or_orphan_request()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = closed_turn(&f, 171).await?;
    let second = closed_turn(&f, 172).await?;
    insert_request(&f, &Req::primary(173, 40, "orphan reply")).await?;
    let mut to_turn = sample_row(first, f.session_id, Utc::now());
    to_turn.next_state = "resolved".to_owned();
    to_turn.next_genuine_turn_id = Some(second);
    to_turn.next_genuine_request_id = Some(172);
    let mut to_orphan = sample_row(second, f.session_id, Utc::now());
    to_orphan.next_state = "resolved".to_owned();
    to_orphan.next_genuine_request_id = Some(173);
    f.handle
        .dossier()
        .upsert_turn_facts(&[to_turn, to_orphan])
        .await?;
    let rows = f
        .handle
        .eval()
        .fetch_judge_turn_contexts(&[first, second])
        .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].next_genuine_text.as_deref(), Some("prompt"));
    assert_eq!(rows[1].next_genuine_text.as_deref(), Some("orphan reply"));
    Ok(())
}

#[tokio::test]
async fn fetch_judge_turn_contexts_for_empty_ids_returns_empty() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(
        f.handle
            .eval()
            .fetch_judge_turn_contexts(&[])
            .await?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn turn_verdicts_cascade_when_turn_deleted() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 181).await?;
    f.handle
        .eval()
        .insert_turn_verdict(legacy(turn, "accepted"))
        .await?;
    sqlx::query("DELETE FROM memory.operator_turns WHERE id = $1")
        .bind(turn)
        .execute(f.handle.pool())
        .await?;
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.turn_verdicts WHERE operator_turn_id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(left, 0);
    Ok(())
}
