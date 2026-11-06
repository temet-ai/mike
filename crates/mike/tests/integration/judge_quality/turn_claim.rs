use super::fixtures::{bootstrap_project_session, seed_unjudged_operator_turn};

use crate::test_db;

use chrono::Utc;
use mike::store::persistence::PersistHandle;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

#[tokio::test]
async fn claim_unjudged_turns_orders_newest_first() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db_handle = test_db.acquire_isolated().await?;

    let persist = PersistHandle::connect(&db_handle.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_handle.database_url)
        .await?;

    let project_path = format!("/test/claim-unjudged/{}", Uuid::new_v4().simple());
    let (project_id, session_pk, _session_uuid) =
        bootstrap_project_session(&pool, &project_path).await?;

    let base = Utc::now();
    let closed_oldest = base;
    let closed_middle = base + chrono::Duration::seconds(20);
    let closed_newest = base + chrono::Duration::seconds(40);

    let turn_oldest =
        seed_unjudged_operator_turn(&pool, session_pk, project_id, closed_oldest).await?;
    let turn_newest =
        seed_unjudged_operator_turn(&pool, session_pk, project_id, closed_newest).await?;
    let turn_middle =
        seed_unjudged_operator_turn(&pool, session_pk, project_id, closed_middle).await?;

    let claimed = persist.claim_unjudged_turns(10, "test-rubric/v1").await?;
    let claimed_ids: Vec<i64> = claimed.iter().map(|row| row.turn_id).collect();

    assert_eq!(
        claimed_ids,
        vec![turn_newest, turn_middle, turn_oldest],
        "claim_unjudged_turns must return newest-closed_at-first; got {claimed_ids:?}"
    );

    db_handle.cleanup().await?;
    Ok(())
}
