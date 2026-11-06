use std::collections::{BTreeSet, HashMap};

use mike::eval::labels::import::fleet_mirror::ImportPlan;
use mike::eval::labels::import::fleet_mirror::rows::SeedRow;
use mike::store::persistence::eval_labels::ImportOutcome;
use serde_json::json;

use crate::turn_facts::resolution::message;
use crate::turn_facts::{Fixture, fixture};

fn seed(turn: i64, axis: &'static str, label: &str) -> SeedRow {
    SeedRow {
        subject_kind: "turn",
        subject_id: turn,
        axis,
        label: label.to_owned(),
        method: "single_model",
        tags: Vec::new(),
        severity: None,
        confidence: Some("high".to_owned()),
        evidence: Some("the operator's words".to_owned()),
        source: "fleet_mirror/census",
        provenance: json!({"fleet_run": "fleet-mirror-7772e9a0", "asset": "census", "row_index": 0}),
        inclusion_prob: Some(1.0),
    }
}

async fn small_plan(f: &Fixture) -> anyhow::Result<ImportPlan> {
    let a = message(f, 9100, 0, "please add a retry", Some(100)).await?;
    let b = message(f, 9101, 100, "and the docs", Some(200)).await?;
    Ok(ImportPlan {
        rows: vec![
            seed(a, "reaction", "accepted"),
            seed(b, "reaction", "corrected"),
        ],
        turns: BTreeSet::from([a, b]),
        session_of_turn: HashMap::new(),
        exclusions: Vec::new(),
        manifest: json!([]),
        counts: json!({"rows": 2}),
    })
}

#[tokio::test]
async fn the_import_writes_its_rows_once_and_a_repeat_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let plan = small_plan(&f).await?;
    let turns: Vec<i64> = plan.turns.iter().copied().collect();
    let resolved = f.handle.resolve_label_turns(&turns).await?;
    assert_eq!(resolved.len(), 2);
    let first = f
        .handle
        .import_fleet_mirror(&plan, &resolved, "test")
        .await?;
    assert!(
        matches!(
            first,
            ImportOutcome::Written {
                labels: 2,
                splits: 2,
                ..
            }
        ),
        "{first:?}"
    );
    let again = f
        .handle
        .import_fleet_mirror(&plan, &resolved, "test")
        .await?;
    assert!(
        matches!(again, ImportOutcome::AlreadyImported { .. }),
        "{again:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.labels")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(rows, 2);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_turn_already_in_holdout_aborts_the_import() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let plan = small_plan(&f).await?;
    let first = *plan
        .turns
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty plan"))?;
    sqlx::query("INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason) VALUES ($1, 'holdout', 'hash')")
        .bind(first)
        .execute(f.handle.pool())
        .await?;
    let turns: Vec<i64> = plan.turns.iter().copied().collect();
    let resolved = f.handle.resolve_label_turns(&turns).await?;
    let outcome = f
        .handle
        .import_fleet_mirror(&plan, &resolved, "test")
        .await?;
    assert_eq!(outcome, ImportOutcome::TurnInHoldout(vec![first]));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.labels")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(rows, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_missing_turn_is_not_resolved() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let resolved = f.handle.resolve_label_turns(&[987_654_321]).await?;
    assert!(
        resolved.is_empty(),
        "a nonexistent census id must be reported missing"
    );
    f.db.cleanup().await?;
    Ok(())
}
