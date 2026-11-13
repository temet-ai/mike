use super::SteererUpdate;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

#[derive(sqlx::FromRow)]
struct Current {
    id: i64,
    display_name: String,
    description: String,
    enabled: bool,
    authority: String,
    mode: String,
    model: String,
    priority: i32,
    trigger_point: String,
    invocation: String,
}

#[tokio::test]
async fn update_steerer_merges_config_keeping_unsent_keys() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let current: Current = sqlx::query_as(
        "SELECT s.id, s.display_name, s.description, s.enabled, a.code AS authority,
                m.code AS mode, s.model, s.priority, s.trigger_point, s.invocation
           FROM steering.steerers s
           JOIN steering.authorities a ON a.id = s.authority_id
           JOIN steering.modes m ON m.id = s.default_mode_id
          ORDER BY s.id LIMIT 1",
    )
    .fetch_one(handle.pool())
    .await?;
    sqlx::query(
        "UPDATE steering.steerers SET config = '{\"rollout\": \"shadow\", \"quiescence_after_secs\": 90}'::jsonb WHERE id = $1",
    )
    .bind(current.id)
    .execute(handle.pool())
    .await?;
    let sent = serde_json::json!({"confidence_threshold": 0.8, "debounce_turns": 2});
    handle
        .steerer()
        .update_steerer(
            SteererUpdate {
                id: current.id,
                display_name: &current.display_name,
                description: &current.description,
                enabled: current.enabled,
                authority_code: &current.authority,
                mode_code: &current.mode,
                model: &current.model,
                priority: current.priority,
                config: &sent,
                prompt: None,
                trigger_point: &current.trigger_point,
                invocation: &current.invocation,
                pattern: None,
                project_id: None,
            },
            None,
        )
        .await?;
    let config: serde_json::Value =
        sqlx::query_scalar("SELECT config FROM steering.steerers WHERE id = $1")
            .bind(current.id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(config["rollout"], "shadow");
    assert_eq!(config["quiescence_after_secs"], 90);
    assert_eq!(config["debounce_turns"], 2);
    db.cleanup().await?;
    Ok(())
}
