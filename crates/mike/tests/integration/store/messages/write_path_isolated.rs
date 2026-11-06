use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

use mike::store::persistence::messages::reads::fetch_predecessor_bridge;
use mike::store::persistence::messages::{canonicalise_body_messages, write_request_messages};

use super::support::{seed_project_session, seed_request, test_pool_isolated};

#[tokio::test]
async fn write_request_messages_carries_forward_without_reinserting() -> anyhow::Result<()> {
    let Some((pool, handle_db)) = test_pool_isolated().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) = seed_project_session(&pool, "carry-proj", &session_id).await?;
    let base = Utc::now();

    let p1 = seed_request(&pool, session_fk, project_id, "primary", base).await?;
    let body1 = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "turn one"},
            {"role": "assistant", "content": "reply one"}
        ]
    }))?;
    let canon1 = canonicalise_body_messages(&body1)?;
    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, p1, &canon1, &HashMap::new()).await?;
    tx.commit().await?;

    let total_after_p1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages")
        .fetch_one(&pool)
        .await?;
    assert_eq!(total_after_p1, 2, "two distinct bodies inserted for p1");

    let mut conn = pool.acquire().await?;
    let bridge = fetch_predecessor_bridge(&mut conn, p1).await?;
    drop(conn);
    let carry: HashMap<String, i64> = bridge
        .iter()
        .map(|(_, sha, id)| (sha.clone(), *id))
        .collect();
    assert_eq!(carry.len(), 2, "both p1 messages available to carry");

    let p2 = seed_request(
        &pool,
        session_fk,
        project_id,
        "primary",
        base + chrono::Duration::seconds(1),
    )
    .await?;
    let body2 = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "turn one"},
            {"role": "assistant", "content": "reply one"},
            {"role": "user", "content": "turn two (new)"}
        ]
    }))?;
    let canon2 = canonicalise_body_messages(&body2)?;
    let mut tx2 = pool.begin().await?;
    write_request_messages(&mut tx2, p2, &canon2, &carry).await?;
    tx2.commit().await?;

    let total_after_p2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        total_after_p2, 3,
        "carry-forward must not re-insert the two in-lineage bodies"
    );

    let p2_bridge = {
        let mut c = pool.acquire().await?;
        fetch_predecessor_bridge(&mut c, p2).await?
    };
    assert_eq!(p2_bridge.len(), 3, "three bridge rows for p2");
    assert_eq!(
        p2_bridge[0].2, carry[&canon2[0].sha],
        "ord 0 carried p1's surrogate id"
    );
    assert_eq!(
        p2_bridge[1].2, carry[&canon2[1].sha],
        "ord 1 carried p1's surrogate id"
    );
    assert!(
        !carry.contains_key(&canon2[2].sha),
        "ord 2 was genuinely new to the lineage"
    );

    handle_db.cleanup().await?;
    Ok(())
}
