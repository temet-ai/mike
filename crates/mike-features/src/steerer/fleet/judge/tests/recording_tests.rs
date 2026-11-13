use std::sync::Arc;

use arc_swap::ArcSwap;

use super::super::*;
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns, seed_two_sessions,
};

#[tokio::test]
async fn same_request_judge_writes_no_row_without_turn_closed_steerer() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id = seed_project(&pool, "j3-unjudged-evaluation").await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(&pool, &session, &spare, project_id).await?;
    seed_session_turns(&pool, &session, project_id, 1).await?;
    let (session_db_id, turn_id): (i64, i64) = sqlx::query_as(
        "SELECT s.id, t.id
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;

    let registry = Arc::new(ArcSwap::from_pointee(SteererRegistry::default()));
    let key_slot: crate::llm::key::OpenRouterKeySlot = Arc::new(ArcSwap::from_pointee(None));
    let llm = OpenRouterClient::new("sk-test".to_string(), "m".to_string(), None, None)?;
    let config = SteererConfig::default();
    let settings: crate::settings::SharedSettingsOverlay = Arc::new(ArcSwap::from_pointee(
        crate::settings::SettingsOverlay::default(),
    ));
    let outcome = judge_turn_close_now(
        JudgeDeps {
            persist: &handle,
            steerer_registry: registry.as_ref(),
            llm: &llm,
            key_slot: &key_slot,
            config: &config,
            settings: &settings,
        },
        &session,
        turn_id,
        "The work has shipped.".to_string(),
        "please finish",
        None,
    )
    .await;
    assert!(
        outcome.fired.is_none(),
        "no steerer was evaluated, so nothing fires"
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM steering.evaluations WHERE session_id = $1")
            .bind(session_db_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(rows, 0, "no steerer was evaluated, so no row is written");

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
