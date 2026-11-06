use chrono::Utc;

use mike::store::models::SystemPromptSource;
use mike::store::persistence::rehydrate::fetch_request_by_id;

use crate::store::rehydrate::support::{
    SeedRequest, seed_agent, seed_agent_instance, seed_project, seed_request, seed_scoped_request,
    seed_session, shared_pool, unique,
};

#[tokio::test]
async fn fetch_request_by_id_null_sha_falls_back_to_main_lane_ledger() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("ledger-main-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let main_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let ledger_blocks = serde_json::json!([{"type": "text", "text": "main-lane-ledger-blocks"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, blocks)
         VALUES ($1, $2, 'main', $3)",
    )
    .bind(session_fk)
    .bind(main_id)
    .bind(&ledger_blocks)
    .execute(&pool)
    .await?;

    let later_primary_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base + chrono::Duration::seconds(1),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let got = fetch_request_by_id(&pool, later_primary_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(
        got.system_prompt.prompt_sha256, None,
        "no class-sha row was seeded"
    );
    assert_eq!(
        got.system_prompt.blocks,
        Some(ledger_blocks),
        "a later primary turn with no own capture row resolves via the \
         session's main-lane ledger row"
    );
    assert_eq!(
        got.system_prompt.source,
        SystemPromptSource::SessionMain,
        "a primary turn legitimately shares the session's main-lane prompt"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_own_capture_row_wins_over_session_main_lane_ledger()
-> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("ledger-own-capture-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let main_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;
    let main_lane_blocks =
        serde_json::json!([{"type": "text", "text": "session-main-lane-blocks"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, blocks)
         VALUES ($1, $2, 'main', $3)",
    )
    .bind(session_fk)
    .bind(main_id)
    .bind(&main_lane_blocks)
    .execute(&pool)
    .await?;

    let subagent_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "subagent",
            created_at: base + chrono::Duration::seconds(1),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;
    let own_capture_blocks =
        serde_json::json!([{"type": "text", "text": "subagent-own-capture-blocks"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, blocks)
         VALUES ($1, $2, 'subagent', $3)",
    )
    .bind(session_fk)
    .bind(subagent_id)
    .bind(&own_capture_blocks)
    .execute(&pool)
    .await?;

    let got = fetch_request_by_id(&pool, subagent_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(
        got.system_prompt.blocks,
        Some(own_capture_blocks),
        "blocks resolve via this request's own session_prompts capture row, \
         not the session's main-lane row from a different request"
    );
    assert_eq!(
        got.system_prompt.source,
        SystemPromptSource::Exact,
        "the request's own capture row is reported as an exact source"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_null_sha_falls_back_to_subagent_instance_ledger() -> anyhow::Result<()>
{
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("ledger-subagent-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let dispatcher_id =
        seed_scoped_request(&pool, session_fk, project_id, "primary", base, None).await?;
    let agent_id = seed_agent(&pool).await?;
    let instance_id = seed_agent_instance(&pool, session_fk, dispatcher_id, agent_id, 0).await?;

    let subagent_first_id = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "subagent",
        base + chrono::Duration::seconds(1),
        Some(instance_id),
    )
    .await?;

    let ledger_blocks =
        serde_json::json!([{"type": "text", "text": "subagent-instance-ledger-blocks"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, instance_id, blocks)
         VALUES ($1, $2, 'subagent', $3, $4)",
    )
    .bind(session_fk)
    .bind(subagent_first_id)
    .bind(instance_id)
    .bind(&ledger_blocks)
    .execute(&pool)
    .await?;

    let subagent_second_id = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "subagent",
        base + chrono::Duration::seconds(2),
        Some(instance_id),
    )
    .await?;

    let got = fetch_request_by_id(&pool, subagent_second_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(
        got.system_prompt.blocks,
        Some(ledger_blocks),
        "a second turn on the same agent instance, with no own capture row, \
         resolves via the instance-keyed ledger row from the first turn, \
         not the session's (nonexistent) main-lane row"
    );
    assert_eq!(
        got.system_prompt.source,
        SystemPromptSource::AgentInstance,
        "an agent-instance-keyed ledger row is reported as such"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_class_sha_wins_over_ledger_fallback() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("ledger-precedence-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let req_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: Utc::now(),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let sha = uuid::Uuid::new_v4().simple().to_string() + &"c".repeat(32);
    let class_sha_blocks = serde_json::json!([{"type": "text", "text": "class-sha-blocks"}]);
    sqlx::query(
        "WITH prompt AS (
             INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
             VALUES ($2, $3)
             RETURNING id
         )
         INSERT INTO requests.system_prompts
            (request_id, prompt_sha256, system_prompt_id, override_applied, filter_modified)
         SELECT $1, $2, prompt.id, false, false FROM prompt",
    )
    .bind(req_id)
    .bind(&sha)
    .bind(&class_sha_blocks)
    .execute(&pool)
    .await?;

    let ledger_blocks = serde_json::json!([{"type": "text", "text": "ledger-blocks"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, blocks)
         VALUES ($1, $2, 'main', $3)",
    )
    .bind(session_fk)
    .bind(req_id)
    .bind(&ledger_blocks)
    .execute(&pool)
    .await?;

    let got = fetch_request_by_id(&pool, req_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(
        got.system_prompt.blocks,
        Some(class_sha_blocks),
        "class-sha exact match wins over the ledger fallback"
    );
    assert_eq!(
        got.system_prompt.source,
        SystemPromptSource::Class,
        "a class-sha exact match is reported as such"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_sidecar_turn_never_borrows_session_main_lane_blocks()
-> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("ledger-sidecar-no-borrow-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let main_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;
    let main_lane_blocks =
        serde_json::json!([{"type": "text", "text": "session-main-lane-blocks-not-for-sidecar"}]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, blocks)
         VALUES ($1, $2, 'main', $3)",
    )
    .bind(session_fk)
    .bind(main_id)
    .bind(&main_lane_blocks)
    .execute(&pool)
    .await?;

    let sidecar_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "quota_probe",
            created_at: base + chrono::Duration::seconds(1),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let got = fetch_request_by_id(&pool, sidecar_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(
        got.system_prompt.blocks, None,
        "a sidecar turn with no own capture row must never display the session's \
         main-lane system prompt from a different request"
    );
    assert_eq!(
        got.system_prompt.source,
        SystemPromptSource::NotCaptured,
        "no source applies when the request has no system-prompt capture of its own"
    );

    pool.close().await;
    Ok(())
}
