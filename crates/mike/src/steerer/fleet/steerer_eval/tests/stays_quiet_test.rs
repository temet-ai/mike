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
async fn verifier_gate_stays_quiet_without_pending_mail() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_project_id, session_uuid, _session_db_id) =
        seed_project_and_session(&pool, "proj-verifier-no-mail").await?;

    let verdict_json = r#"{"fires":false,"confidence":0.9,"reason":"looks complete","nudge":""}"#;
    let (_openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store.clone(), Some(handle), llm);

    let eval_inputs = EvalInputs {
        eval_text: "I've completed the task, everything is done.".to_string(),
        operator_ask: "please finish this up".to_string(),
        last_user_message: "please finish this up".to_string(),
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };
    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(1),
        eval_inputs,
    )
    .await;

    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert!(
        reminders.is_empty(),
        "no mailbox row is pending — verdict.fires=false must NOT be overridden"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
