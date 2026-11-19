use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

use mike::store::persistence::messages::{
    canonicalise_body_messages, compute_delta, write_request_deltas, write_request_messages,
};

use super::support::{seed_project_session, seed_request, test_pool};

async fn row_versions(pool: &sqlx::PgPool, req_id: i64) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT xmin::text FROM requests.deltas WHERE request_id = $1 ORDER BY ord",
    )
    .bind(req_id)
    .fetch_all(pool)
    .await?)
}

#[tokio::test]
async fn write_request_deltas_leaves_identical_rows_alone() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("dr-proj-{session_id}"), &session_id).await?;
    let req_id = seed_request(&pool, session_fk, project_id, "primary", Utc::now()).await?;
    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "one"},
            {"role": "assistant", "content": "two"}
        ]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;
    let rows = compute_delta(&canonical, None);

    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, req_id, &canonical, &HashMap::new()).await?;
    write_request_deltas(&mut tx, req_id, None, session_fk, &rows).await?;
    tx.commit().await?;
    let first = row_versions(&pool, req_id).await?;
    assert_eq!(first.len(), 2);

    let mut tx = pool.begin().await?;
    write_request_deltas(&mut tx, req_id, None, session_fk, &rows).await?;
    tx.commit().await?;
    assert_eq!(
        row_versions(&pool, req_id).await?,
        first,
        "an identical second write must not delete and reinsert the rows"
    );

    let mut tx = pool.begin().await?;
    write_request_deltas(&mut tx, req_id, None, session_fk, &rows[..1]).await?;
    tx.commit().await?;
    let changed = row_versions(&pool, req_id).await?;
    assert_eq!(
        changed.len(),
        1,
        "a different set of rows still replaces the old one"
    );
    Ok(())
}
