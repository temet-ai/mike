use chrono::Utc;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::HashMap;

use mike::store::persistence::messages::reads::{
    fetch_predecessor_bridge, fetch_predecessor_primary,
};
use mike::store::persistence::messages::{
    canonicalise_body_messages, compute_delta, fetch_request_deltas_by_id, write_request_deltas,
    write_request_messages,
};

use super::support::{seed_project_session, seed_request, test_pool};

#[tokio::test]
async fn write_request_messages_resolves_message_id_via_sha_join_and_is_idempotent()
-> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("wrm-proj-{session_id}"), &session_id).await?;
    let req_id = seed_request(&pool, session_fk, project_id, "primary", Utc::now()).await?;

    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"}
        ]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;
    assert_eq!(canonical.len(), 2, "two messages canonicalised");

    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, req_id, &canonical, &HashMap::new()).await?;
    tx.commit().await?;

    let mismatches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
               FROM requests.messages rm
               LEFT JOIN messages.messages m ON m.id = rm.message_id
              WHERE rm.request_id = $1
                AND m.id IS NULL",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        mismatches, 0,
        "every bridge message_id must resolve to messages.id"
    );

    let bridge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM requests.messages WHERE request_id = $1")
            .bind(req_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(bridge_count, 2, "one bridge row per message ord");

    let mut tx2 = pool.begin().await?;
    write_request_messages(&mut tx2, req_id, &canonical, &HashMap::new()).await?;
    tx2.commit().await?;
    let bridge_count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM requests.messages WHERE request_id = $1")
            .bind(req_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        bridge_count_after, 2,
        "re-run must not duplicate bridge rows"
    );

    Ok(())
}

#[tokio::test]
async fn write_request_messages_persists_cache_control_per_row() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("wrm-cc-{session_id}"), &session_id).await?;
    let req_id = seed_request(&pool, session_fk, project_id, "primary", Utc::now()).await?;

    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi", "cache_control": {"type": "ephemeral"}}
        ]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;

    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, req_id, &canonical, &HashMap::new()).await?;
    tx.commit().await?;

    let rows: Vec<(i32, Option<JsonValue>)> = sqlx::query_as(
        "SELECT ord, cache_control FROM requests.messages
              WHERE request_id = $1 ORDER BY ord ASC",
    )
    .bind(req_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].1, None,
        "ord 0 carried no cache_control on the wire"
    );
    assert_eq!(
        rows[1].1,
        Some(json!([{"pointer": "/cache_control", "value": {"type": "ephemeral"}}])),
        "ord 1's breakpoint must land on ord 1's own row"
    );

    Ok(())
}

#[tokio::test]
async fn predecessor_primary_and_bridge_real_fns() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("pp-proj-{session_id}"), &session_id).await?;
    let base = Utc::now();

    let p1 = seed_request(&pool, session_fk, project_id, "primary", base).await?;

    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "q"},
            {"role": "assistant", "content": "a"}
        ]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;
    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, p1, &canonical, &HashMap::new()).await?;
    tx.commit().await?;

    let mut conn = pool.acquire().await?;
    let pred = fetch_predecessor_primary(
        &mut conn,
        session_fk,
        base + chrono::Duration::seconds(1),
        None,
    )
    .await?;
    let (pred_id, _ts) = pred.ok_or_else(|| anyhow::anyhow!("expected p1 as predecessor"))?;
    assert_eq!(pred_id, p1, "chrono-prior primary resolves to p1");

    let bridge = fetch_predecessor_bridge(&mut conn, pred_id).await?;
    assert_eq!(bridge.len(), 2, "bridge has both messages");
    assert_eq!(bridge[0].0, 0, "ord 0 first");
    assert_eq!(bridge[1].0, 1, "ord 1 second");
    assert_eq!(bridge[0].1, canonical[0].sha, "sha matches canonical ord 0");
    assert_eq!(bridge[1].1, canonical[1].sha, "sha matches canonical ord 1");
    assert!(
        bridge[0].2 > 0 && bridge[1].2 > 0,
        "message_id projected from the bridge for carry-forward"
    );
    assert_ne!(
        bridge[0].2, bridge[1].2,
        "two distinct bodies resolve to two distinct surrogate ids"
    );

    let none_pred = fetch_predecessor_primary(&mut conn, session_fk, base, None).await?;
    assert!(
        none_pred.is_none(),
        "no predecessor before the first request"
    );

    Ok(())
}

#[tokio::test]
async fn write_and_fetch_request_deltas_resolves_session_fk() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("wd-proj-{session_id}"), &session_id).await?;
    let req_id = seed_request(&pool, session_fk, project_id, "primary", Utc::now()).await?;

    let body = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": "only message"}]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;
    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, req_id, &canonical, &HashMap::new()).await?;

    let delta_rows = compute_delta(&canonical, None);
    assert_eq!(delta_rows.len(), 1);
    write_request_deltas(&mut tx, req_id, None, session_fk, &delta_rows).await?;
    tx.commit().await?;

    let stored_session_fk: i64 =
        sqlx::query_scalar("SELECT session_id FROM requests.deltas WHERE request_id = $1 LIMIT 1")
            .bind(req_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        stored_session_fk, session_fk,
        "request_deltas.session_id must resolve to the bigint sessions.id"
    );

    let read = fetch_request_deltas_by_id(&pool, req_id, session_fk).await?;
    assert_eq!(read.len(), 1, "one delta row read back");
    let row = &read[0];
    assert_eq!(row.change_kind, "added");
    assert_eq!(row.ord, 0);
    assert_eq!(row.role, "user", "role sourced via JOIN on message_id");
    assert_eq!(
        row.byte_size as usize, canonical[0].byte_size as usize,
        "byte_size sourced via JOIN on message_id"
    );
    assert_eq!(row.message_sha256, canonical[0].sha);
    assert!(row.removed_reason.is_none(), "added rows carry no reason");

    Ok(())
}

#[tokio::test]
async fn write_request_deltas_resolves_message_id_via_bridge_not_sha() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_fk) =
        seed_project_session(&pool, &format!("wrd-bridge-proj-{session_id}"), &session_id).await?;
    let base = Utc::now();

    let pred_id = seed_request(&pool, session_fk, project_id, "primary", base).await?;
    let pred_body = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": "predecessor message"}]
    }))?;
    let pred_canonical = canonicalise_body_messages(&pred_body)?;
    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, pred_id, &pred_canonical, &HashMap::new()).await?;
    tx.commit().await?;

    let cur_id = seed_request(
        &pool,
        session_fk,
        project_id,
        "primary",
        base + chrono::Duration::seconds(1),
    )
    .await?;
    let cur_body = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": "current message (different content)"}]
    }))?;
    let cur_canonical = canonicalise_body_messages(&cur_body)?;
    let mut tx2 = pool.begin().await?;
    write_request_messages(&mut tx2, cur_id, &cur_canonical, &HashMap::new()).await?;

    let pred_bridge: Vec<(i32, String)> = vec![(0, pred_canonical[0].sha.clone())];
    let delta_rows = compute_delta(&cur_canonical, Some(pred_bridge.as_slice()));
    assert_eq!(delta_rows.len(), 2, "one added + one removed");
    assert!(
        delta_rows.iter().any(|r| r.change_kind == "added"),
        "must have added row"
    );
    assert!(
        delta_rows.iter().any(|r| r.change_kind == "removed"),
        "must have removed row"
    );

    write_request_deltas(&mut tx2, cur_id, Some(pred_id), session_fk, &delta_rows).await?;
    tx2.commit().await?;

    let persisted = fetch_request_deltas_by_id(&pool, cur_id, session_fk).await?;
    assert_eq!(persisted.len(), 2, "two delta rows persisted");

    let added_row = persisted
        .iter()
        .find(|r| r.change_kind == "added")
        .ok_or_else(|| anyhow::anyhow!("added row missing"))?;
    let cur_msg_id: i64 = sqlx::query_scalar(
        "SELECT message_id FROM requests.messages WHERE request_id = $1 AND ord = 0",
    )
    .bind(cur_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        added_row.message_id, cur_msg_id,
        "added row message_id must resolve from the CURRENT request's bridge"
    );

    let removed_row = persisted
        .iter()
        .find(|r| r.change_kind == "removed")
        .ok_or_else(|| anyhow::anyhow!("removed row missing"))?;
    let pred_msg_id: i64 = sqlx::query_scalar(
        "SELECT message_id FROM requests.messages WHERE request_id = $1 AND ord = 0",
    )
    .bind(pred_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        removed_row.message_id, pred_msg_id,
        "removed row message_id must resolve from the PREDECESSOR request's bridge"
    );

    assert_eq!(added_row.message_sha256, cur_canonical[0].sha);
    assert_eq!(removed_row.message_sha256, pred_canonical[0].sha);

    Ok(())
}

#[tokio::test]
async fn fetch_request_deltas_empty_for_request_without_deltas() -> anyhow::Result<()> {
    let Some((pool, _handle_db)) = test_pool().await? else {
        return Ok(());
    };
    let rows = fetch_request_deltas_by_id(&pool, 9_999_999, 9_999_999).await?;
    assert!(rows.is_empty(), "unknown request id => empty delta list");
    Ok(())
}
