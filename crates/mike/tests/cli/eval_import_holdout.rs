use mike::eval::labels::import::holdout::plan;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::eval_labels::{HoldoutOutcome, SplitConflictPolicy};
use mike::store::test_support::{self, TestDbHandle};
use serde_json::json;
use sha2::{Digest, Sha256};

const AXES: [&str; 5] = [
    "reaction",
    "delivery",
    "fact_own_prompt_corrects_prior",
    "fact_assistant_self_correction",
    "fact_irreversible_action",
];

struct Env {
    db: TestDbHandle,
    handle: PersistHandle,
    turns: Vec<i64>,
    dir: tempfile::TempDir,
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn label_for(axis: &str) -> &'static str {
    match axis {
        "reaction" => "accepted",
        "delivery" => "delivered",
        _ => "false",
    }
}

fn write_batch(dir: &std::path::Path, turns: &[i64], batch: &str) -> anyhow::Result<()> {
    let blind = serde_json::to_vec(
        &turns
            .iter()
            .map(|t| json!({"turn_id": t}))
            .collect::<Vec<_>>(),
    )?;
    std::fs::write(dir.join("sample-blind.json"), &blind)?;
    let manifest =
        json!({"turn_ids": turns, "blind_sha256": sha(&blind), "rubric_sha256": "rubric-v1"});
    std::fs::write(dir.join("manifest.json"), manifest.to_string())?;
    let mut lines = Vec::new();
    for turn in turns {
        for axis in AXES {
            lines.push(
                json!({"subject_kind": "turn", "subject_id": turn, "operator_turn_id": turn, "axis": axis,
                       "label": label_for(axis), "tier": "seed", "method": "double_blind_adjudicated",
                       "split": "holdout", "source": format!("jc11/{batch}"), "confidence": "high",
                       "evidence": "thanks", "inclusion_prob": 1.0,
                       "provenance": {"rubric_sha256": "rubric-v1", "stratum": "replied"}})
                .to_string(),
            );
        }
    }
    std::fs::write(dir.join("import.jsonl"), lines.join("\n"))?;
    Ok(())
}

async fn env(n: i64, batch: &str) -> anyhow::Result<Option<Env>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = test_support::seed_project(handle.pool(), "jc11-holdout").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(handle.pool(), &session, &spare, project).await?;
    test_support::seed_session_turns(handle.pool(), &session, project, n).await?;
    let turns: Vec<i64> = sqlx::query_scalar(
        "SELECT t.id FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid ORDER BY t.id",
    )
    .bind(&session)
    .fetch_all(handle.pool())
    .await?;
    let dir = tempfile::tempdir()?;
    write_batch(dir.path(), &turns, batch)?;
    Ok(Some(Env {
        db,
        handle,
        turns,
        dir,
    }))
}

async fn label_count(handle: &PersistHandle) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM eval.labels")
        .fetch_one(handle.pool())
        .await?)
}

#[tokio::test]
async fn import_holdout_dry_run_writes_nothing_and_twice_is_a_no_op() -> anyhow::Result<()> {
    let Some(e) = env(3, "b1").await? else {
        return Ok(());
    };
    let p = plan(e.dir.path(), "b1").map_err(anyhow::Error::msg)?;
    assert_eq!(p.rows.len(), 15);
    assert_eq!(label_count(&e.handle).await?, 0);
    let resolved = e.handle.resolve_label_turns(&e.turns).await?;
    let first = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::Refuse)
        .await?;
    assert!(
        matches!(first, HoldoutOutcome::Written { labels: 15, .. }),
        "{first:?}"
    );
    let second = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::Refuse)
        .await?;
    assert!(
        matches!(second, HoldoutOutcome::AlreadyImported { .. }),
        "{second:?}"
    );
    assert_eq!(label_count(&e.handle).await?, 15);
    e.db.cleanup().await?;
    Ok(())
}

async fn mark_dev(handle: &PersistHandle, turn: i64) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason) VALUES ($1, 'dev', 'hash')")
        .bind(turn)
        .execute(handle.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn import_holdout_aborts_on_a_dev_split_turn() -> anyhow::Result<()> {
    let Some(e) = env(3, "b2").await? else {
        return Ok(());
    };
    mark_dev(&e.handle, e.turns[0]).await?;
    let p = plan(e.dir.path(), "b2").map_err(anyhow::Error::msg)?;
    let resolved = e.handle.resolve_label_turns(&e.turns).await?;
    let outcome = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::Refuse)
        .await?;
    assert_eq!(outcome, HoldoutOutcome::DevSplitConflict(vec![e.turns[0]]));
    assert_eq!(label_count(&e.handle).await?, 0);
    e.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn import_holdout_drop_split_conflicts_lists_dropped_turns() -> anyhow::Result<()> {
    let Some(e) = env(3, "b3").await? else {
        return Ok(());
    };
    mark_dev(&e.handle, e.turns[0]).await?;
    let p = plan(e.dir.path(), "b3").map_err(anyhow::Error::msg)?;
    let resolved = e.handle.resolve_label_turns(&e.turns).await?;
    let outcome = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::Drop)
        .await?;
    let HoldoutOutcome::Written {
        labels, dropped, ..
    } = outcome
    else {
        anyhow::bail!("expected a written import");
    };
    assert_eq!(labels, 10);
    assert_eq!(dropped, vec![e.turns[0]]);
    let manifest: serde_json::Value =
        sqlx::query_scalar("SELECT manifest FROM eval.import_batches WHERE source_key = 'b3'")
            .fetch_one(e.handle.pool())
            .await?;
    assert_eq!(manifest["dropped_split_conflicts"], json!([e.turns[0]]));
    e.db.cleanup().await?;
    Ok(())
}

async fn mark_used_by_run(handle: &PersistHandle, turn: i64) -> anyhow::Result<()> {
    let bundle: i64 = sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, origin, status, components, content_sha256, build_sha)
         VALUES ('turn_judge', 'candidate', 'candidate', '{}'::jsonb, 'x', 'test') RETURNING id",
    )
    .fetch_one(handle.pool())
    .await?;
    let run: i64 = sqlx::query_scalar(
        "INSERT INTO eval.runs (purpose, bundle_id, rubric_version, input_builder_version, model,
             split, tiers, labels_as_of, label_count, label_ids_hash, max_usd, argv)
         VALUES ('measure', $1, 'v2', 'b', 'm', 'dev', ARRAY['seed'], now(), 1, 'h', 1.0, '[]'::jsonb)
         RETURNING id",
    )
    .bind(bundle)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO eval.run_items (run_id, operator_turn_id, outcome) VALUES ($1, $2, 'refused')",
    )
    .bind(run)
    .bind(turn)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn split_of(handle: &PersistHandle, turn: i64) -> anyhow::Result<(String, String)> {
    Ok(sqlx::query_as(
        "SELECT split, split_reason FROM eval.turn_splits WHERE operator_turn_id = $1",
    )
    .bind(turn)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn import_holdout_resplits_a_dev_turn_no_run_used() -> anyhow::Result<()> {
    let Some(e) = env(3, "b4").await? else {
        return Ok(());
    };
    mark_dev(&e.handle, e.turns[0]).await?;
    let p = plan(e.dir.path(), "b4").map_err(anyhow::Error::msg)?;
    let resolved = e.handle.resolve_label_turns(&e.turns).await?;
    let outcome = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::ResplitUnusedDev)
        .await?;
    assert!(
        matches!(&outcome, HoldoutOutcome::Written { labels: 15, dropped, .. } if dropped.is_empty()),
        "{outcome:?}"
    );
    let (split, reason) = split_of(&e.handle, e.turns[0]).await?;
    assert_eq!(split, "holdout");
    assert_eq!(reason, "holdout_resplit_unused_dev");
    let manifest: serde_json::Value =
        sqlx::query_scalar("SELECT manifest FROM eval.import_batches WHERE source_key = 'b4'")
            .fetch_one(e.handle.pool())
            .await?;
    assert_eq!(manifest["resplit_unused_dev"], json!([e.turns[0]]));
    e.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn import_holdout_refuses_to_resplit_a_turn_an_eval_run_used() -> anyhow::Result<()> {
    let Some(e) = env(3, "b5").await? else {
        return Ok(());
    };
    mark_dev(&e.handle, e.turns[0]).await?;
    mark_used_by_run(&e.handle, e.turns[0]).await?;
    let p = plan(e.dir.path(), "b5").map_err(anyhow::Error::msg)?;
    let resolved = e.handle.resolve_label_turns(&e.turns).await?;
    let outcome = e
        .handle
        .import_holdout(&p, &resolved, "test", SplitConflictPolicy::ResplitUnusedDev)
        .await?;
    assert_eq!(outcome, HoldoutOutcome::DevLabelsInUse(vec![e.turns[0]]));
    assert_eq!(label_count(&e.handle).await?, 0);
    assert_eq!(split_of(&e.handle, e.turns[0]).await?.0, "dev");
    e.db.cleanup().await?;
    Ok(())
}
