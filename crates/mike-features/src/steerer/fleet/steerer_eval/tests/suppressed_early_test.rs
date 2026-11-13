use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{
    evaluate_turn, make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning,
    seed_project_and_session,
};
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

#[tokio::test]
async fn verifier_suppressed_in_early_session_skips_the_llm_call_entirely() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_project_id, session_uuid, _session_db_id) =
        seed_project_and_session(&pool, "proj-verifier-early-suppression").await?;

    let verdict_json =
        r#"{"fires":true,"confidence":0.95,"reason":"would fire if called","nudge":"x"}"#;
    let (openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store.clone(), Some(handle), llm);

    let eval_inputs = EvalInputs {
        eval_text: "Still working through the refactor, a few files left.".to_string(),
        operator_ask: "please refactor the auth module".to_string(),
        last_user_message: "please refactor the auth module".to_string(),
        trigger_point: "response",
        ..Default::default()
    };
    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(1),
        eval_inputs,
    )
    .await;

    assert_eq!(
        openrouter_mock
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        0,
        "arming SUPPRESSED must skip the LLM call entirely, not just decline to fire"
    );
    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert!(
        reminders.is_empty(),
        "a suppressed evaluation fires nothing"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
