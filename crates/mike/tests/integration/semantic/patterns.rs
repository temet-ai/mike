use chrono::{Duration, NaiveDate, Utc};
use mike::store::persistence::semantic::{CandidateDecision, NewCandidate, NewDriftRow};

use super::support::{build_all, closed_turn, embedder, embedding_stub, facts, service};
use crate::turn_facts::fixture;

#[tokio::test]
async fn pattern_candidate_naming_inserts_axis_label_and_member_anchors_in_one_transaction()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let stub = embedding_stub().await;
    let svc = service(&stub.uri());
    for (i, prompt) in [
        "ran the wrong suite",
        "ran the old suite",
        "the suite was wrong",
    ]
    .iter()
    .enumerate()
    {
        let turn = closed_turn(
            &f,
            30_300 + i64::try_from(i)?,
            prompt,
            i64::try_from(i)? * 50,
        )
        .await?;
        facts(&f, turn, None).await?;
    }
    let e = embedder(&f, &svc).await?;
    build_all(&f, &svc, &e).await?;
    let members: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM memory.semantic_items WHERE kind = 'turn_prompt' AND embedder_key = $1 ORDER BY id",
    )
    .bind(&e.embedder_key)
    .fetch_all(f.handle.pool())
    .await?;
    let ids = f
        .handle
        .insert_pattern_candidates(&[NewCandidate {
            cluster_version: "cluster-v1".to_owned(),
            embedder_key: e.embedder_key.clone(),
            floor_id: None,
            window_start: Utc::now() - Duration::days(30),
            window_end: Utc::now(),
            member_item_ids: members.clone(),
            session_count: 1,
            medoid_item_id: members.first().copied(),
        }])
        .await?;
    let before: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(sort_order), 0) FROM eval.axis_labels WHERE axis = 'failure_tag' AND sort_order < 99",
    )
    .fetch_one(f.handle.pool())
    .await?;
    let decision = f
        .handle
        .name_pattern_candidate(
            ids[0],
            "ran-wrong-suite",
            "Ran a different suite than the gate runs.",
            "semantic-items-v1",
        )
        .await?;
    let CandidateDecision::Named {
        sort_order,
        anchor_ids,
    } = decision
    else {
        anyhow::bail!("expected a named decision, got {decision:?}");
    };
    assert_eq!(sort_order, before + 1);
    assert_eq!(anchor_ids.len(), members.len());
    let again = f.handle.dismiss_pattern_candidate(ids[0], "late").await?;
    assert!(matches!(again, CandidateDecision::NotOpen(ref s) if s == "named"));
    Ok(())
}

#[tokio::test]
async fn drift_rows_unique_per_day_stream_key_version() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let stub = embedding_stub().await;
    let e = embedder(&f, &service(&stub.uri())).await?;
    let row = NewDriftRow {
        day: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap_or_default(),
        stream: "prompt",
        embedder_key: &e.embedder_key,
        drift_version: "drift-v1",
        n_current: 40,
        n_baseline: 200,
        distance: Some(0.02),
        z: Some(1.0),
        prior_days: 28,
        prior_mean: Some(0.01),
        prior_sd: Some(0.01),
        status: "scored",
    };
    assert!(f.handle.insert_drift_row(&row).await?.is_some());
    assert!(f.handle.insert_drift_row(&row).await?.is_none());
    Ok(())
}
