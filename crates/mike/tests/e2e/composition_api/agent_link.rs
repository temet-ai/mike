use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{create_via_api, prompt_sha256_for};

async fn fetch_linked_agent_uuid(
    pool: &sqlx::PgPool,
    composition_id: i64,
) -> anyhow::Result<Option<uuid::Uuid>> {
    let uuid: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT a.uuid
               FROM overrides.overrides pc
               LEFT JOIN agents.agents a ON a.id = pc.agent_id
              WHERE pc.id = $1",
    )
    .bind(composition_id)
    .fetch_one(pool)
    .await?;
    Ok(uuid)
}

#[tokio::test]
async fn patch_agent_name_resolves_agent_id_link() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("patch-agent-link-block2");
    let envelope = create_via_api(&client, &inspect_url, &prompt_sha256, "agent-link", "x").await?;
    let composition_id_str = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent"))?
        .to_string();
    let composition_id_int: i64 = composition_id_str.parse()?;

    assert!(envelope["composition"]["agent_name"].is_null());
    let agent_id_before = fetch_linked_agent_uuid(&pool, composition_id_int).await?;
    assert!(
        agent_id_before.is_none(),
        "agent_id must be NULL before linking; got {agent_id_before:?}"
    );

    let link_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id_str}"
        ))
        .json(&json!({
            "agent_name": "researcher-test-agent",
            "agent_definition_path": "/tmp/researcher-test.md"
        }))
        .send()
        .await?;
    assert_eq!(link_resp.status(), 200);
    let link_body: Value = link_resp.json().await?;
    assert_eq!(
        link_body["composition"]["agent_name"].as_str(),
        Some("researcher-test-agent"),
    );
    assert_eq!(
        link_body["composition"]["agent_definition_path"].as_str(),
        Some("/tmp/researcher-test.md"),
    );

    let (agent_uuid, agent_path): (uuid::Uuid, Option<String>) =
        sqlx::query_as("SELECT uuid, definition_path FROM agents.agents WHERE name = $1")
            .bind("researcher-test-agent")
            .fetch_one(&pool)
            .await?;
    assert_eq!(agent_path.as_deref(), Some("/tmp/researcher-test.md"));

    let agent_id_after = fetch_linked_agent_uuid(&pool, composition_id_int).await?;
    assert_eq!(
        agent_id_after,
        Some(agent_uuid),
        "agent_id FK must point at the find-or-created agent row"
    );

    let clear_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id_str}"
        ))
        .json(&json!({"agent_name": Value::Null}))
        .send()
        .await?;
    assert_eq!(clear_resp.status(), 200);
    let clear_body: Value = clear_resp.json().await?;
    assert!(clear_body["composition"]["agent_name"].is_null());
    let agent_id_cleared = fetch_linked_agent_uuid(&pool, composition_id_int).await?;
    assert!(
        agent_id_cleared.is_none(),
        "clearing agent_name must null agent_id; got {agent_id_cleared:?}"
    );

    let relink_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id_str}"
        ))
        .json(&json!({"agent_name": "researcher-test-agent"}))
        .send()
        .await?;
    assert_eq!(relink_resp.status(), 200);
    let agent_id_relinked = fetch_linked_agent_uuid(&pool, composition_id_int).await?;
    assert_eq!(
        agent_id_relinked,
        Some(agent_uuid),
        "re-link by name alone must reuse the existing agents row"
    );
    let path_after_relink: Option<String> =
        sqlx::query_scalar("SELECT definition_path FROM agents.agents WHERE uuid = $1")
            .bind(agent_uuid)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        path_after_relink.as_deref(),
        Some("/tmp/researcher-test.md"),
        "find-or-create must not stomp the existing definition_path"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
