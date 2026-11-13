use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::Value;

use crate::turn_facts::{Fixture, fixture};

const MIGRATION: &str =
    include_str!("../../../src/store/persistence/migrations/20260925000600_turn_guard.sql");

async fn guard_id(f: &Fixture) -> anyhow::Result<i64> {
    Ok(f.handle
        .steerer()
        .guard_steerer()
        .await?
        .ok_or_else(|| anyhow::anyhow!("turn-guard row not seeded"))?
        .id)
}

#[tokio::test]
async fn migration_seeds_guard_row_disabled_with_shadow_rollout_and_four_shadow_policies_idempotently()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let Some((_, seeds)) = MIGRATION.split_once("INSERT INTO agents.agents") else {
        anyhow::bail!("the turn guard migration no longer seeds the agent row");
    };
    let rerun = format!("INSERT INTO agents.agents{seeds}");
    sqlx::raw_sql(sqlx::AssertSqlSafe(rerun))
        .execute(f.handle.pool())
        .await?;
    let (count, enabled, config): (i64, bool, Value) = sqlx::query_as(
        "SELECT count(*) OVER (), enabled, config FROM steering.steerers WHERE name = 'turn-guard'",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(count, 1);
    assert!(!enabled);
    assert_eq!(config["rollout"], "shadow");
    assert!(
        config["questions"]["relapse"].get("bar").is_none(),
        "bars are code constants"
    );
    let policies = f
        .handle
        .steerer()
        .question_policies(guard_id(&f).await?)
        .await?;
    assert_eq!(policies.len(), 4);
    assert!(policies.iter().all(|p| p.mode == "shadow"));
    Ok(())
}

#[tokio::test]
async fn handback_wake_rejected_by_check() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    let err = f
        .handle
        .steerer()
        .set_question_mode(id, "authorised_handback", "wake", "cli")
        .await;
    assert!(
        err.is_err(),
        "question_policies_handback_never_wakes must refuse"
    );
    assert!(
        f.handle
            .steerer()
            .set_question_mode(id, "relapse", "off", "cli")
            .await
            .is_ok()
    );
    Ok(())
}

#[tokio::test]
async fn turn_guard_kind_accepted_by_check_and_unknown_kind_still_rejected() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let update = |kind: &'static str| {
        sqlx::query("UPDATE steering.steerers SET kind = $1 WHERE name = 'turn-guard'").bind(kind)
    };
    let mut tx = f.handle.pool().begin().await?;
    assert!(update("turn_guard").execute(&mut *tx).await.is_ok());
    tx.rollback().await?;
    let mut tx = f.handle.pool().begin().await?;
    assert!(update("future_kind").execute(&mut *tx).await.is_err());
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn question_policy_patch_rejects_append_and_wake() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let base = crate::session_end::frustration::serve(&f).await?;
    let client = reqwest::Client::new();
    for mode in ["append", "wake"] {
        let resp = client
            .patch(format!("{base}/api/steering/guard/questions/relapse"))
            .json(&serde_json::json!({"mode": mode}))
            .send()
            .await?;
        assert_eq!(resp.status().as_u16(), 409, "{mode}");
    }
    let ok = client
        .patch(format!("{base}/api/steering/guard/questions/relapse"))
        .json(&serde_json::json!({"mode": "off"}))
        .send()
        .await?;
    assert_eq!(ok.status().as_u16(), 200);
    let body: Value = ok.json().await?;
    assert_eq!(body["mode"], "off");
    assert_eq!(body["updated_via"], "spa");
    Ok(())
}
