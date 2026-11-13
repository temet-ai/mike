use mike::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use mike::store::persistence::session_flow::{FlowAvailability, FlowComponent};

use super::support::{Fixture, ids, no_raw};

#[tokio::test]
async fn result_carrier_scope_preserves_emitter_identity_without_peer_expansion()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let carrier = f.request(None, "primary").await?;
    let peer = Uuid::new_v4();
    let peer_id = mike::store::persistence::id_resolvers::resolve_session_id(
        f.handle.pool(),
        &peer.to_string(),
        None,
        None,
    )
    .await?;
    let emitter_request: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
        (session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
         status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id)
        SELECT $2, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
               status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id
        FROM requests.requests WHERE id = $1 RETURNING id",
    )
    .bind(carrier)
    .bind(peer_id)
    .fetch_one(f.handle.pool())
    .await?;
    let tool: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name)
        VALUES ($1, $2, 0, 'cross-session-tool', 'Bash') RETURNING id",
    )
    .bind(emitter_request)
    .bind(peer_id)
    .fetch_one(f.handle.pool())
    .await?;
    let result: i64 = sqlx::query_scalar("INSERT INTO tools.results (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes)
        VALUES ($1, $2, $3, 0, false, '\"RAW_SENTINEL\"', 'fingerprint', 12) RETURNING id")
        .bind(carrier).bind(f.id).bind(tool).fetch_one(f.handle.pool()).await?;
    assert!(
        f.page(FlowComponent::Tools, 10, None)
            .await?
            .activities
            .is_empty()
    );
    let unavailable = f
        .handle
        .web()
        .fetch_session_flow(peer, FlowComponent::Results, 10, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("peer missing"))?;
    assert_eq!(unavailable.availability, FlowAvailability::NoAdmittedStart);
    let page = f.page(FlowComponent::Results, 10, None).await?;
    assert_eq!(ids(&page), vec![result]);
    assert_eq!(page.activities[0].source.session_id, Some(f.session));
    assert_eq!(page.activities[0].source.request_id, Some(carrier));
    assert_eq!(page.actors[0].session_id, Some(peer));
    assert_eq!(page.actors[0].id, format!("tool:{peer}:{tool}"));
    assert_eq!(page.actors[0].label, "Bash");
    assert_eq!(page.actors.len(), 1);
    no_raw(&page);
    f.cleanup().await
}
