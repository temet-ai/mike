use chrono::Utc;

use mike::store::persistence::rehydrate::fetch_request_by_id;

use crate::store::rehydrate::support::{
    SeedRequest, isolated_pool, seed_project, seed_request, seed_session, shared_pool, unique,
};

async fn seed_composition_chain(pool: &sqlx::PgPool, req_id: i64) -> anyhow::Result<(i64, i64)> {
    let sha = uuid::Uuid::new_v4().simple().to_string() + &"b".repeat(32);
    let system_prompt_id: i64 = sqlx::query_scalar(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
         VALUES ($1, '[{\"type\":\"text\",\"text\":\"sys\"}]'::jsonb)
         RETURNING id",
    )
    .bind(&sha)
    .fetch_one(pool)
    .await?;
    let comp_uuid = uuid::Uuid::new_v4();
    let comp_id: i64 = sqlx::query_scalar(
        "INSERT INTO overrides.overrides
            (uuid, prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, $2, $3, 'lbl', '[]'::jsonb)
         RETURNING id",
    )
    .bind(comp_uuid)
    .bind(&sha)
    .bind(system_prompt_id)
    .fetch_one(pool)
    .await?;
    let rsp_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.system_prompts
            (request_id, prompt_sha256, override_id, system_prompt_id,
             override_applied, filter_modified)
         VALUES ($1, $2, $3, $4, true, true)
         RETURNING id",
    )
    .bind(req_id)
    .bind(&sha)
    .bind(comp_id)
    .bind(system_prompt_id)
    .fetch_one(pool)
    .await?;
    Ok((rsp_id, comp_id))
}

async fn seed_firing_rules(pool: &sqlx::PgPool, rsp_id: i64) -> anyhow::Result<(i64, i64)> {
    let rule_first = uuid::Uuid::new_v4();
    let rule_second = uuid::Uuid::new_v4();
    let fr_first: i64 = sqlx::query_scalar(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, enabled, source_id)
         SELECT $1, rk.id, 'aaa', ra.id, true, rs.id
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'text_substring' AND ra.code = 'drop' AND rs.code = 'toml'
         RETURNING id",
    )
    .bind(rule_first)
    .fetch_one(pool)
    .await?;
    let fr_second: i64 = sqlx::query_scalar(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, enabled, source_id)
         SELECT $1, rk.id, 'bbb', ra.id, true, rs.id
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'text_substring' AND ra.code = 'drop' AND rs.code = 'toml'
         RETURNING id",
    )
    .bind(rule_second)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO requests.system_prompt_firing_rules
            (request_system_prompt_id, filter_rule_id, ord)
         VALUES ($1, $2, 1), ($1, $3, 0)",
    )
    .bind(rsp_id)
    .bind(fr_second)
    .bind(fr_first)
    .execute(pool)
    .await?;
    Ok((fr_first, fr_second))
}

async fn seed_declared_and_invoked_tool(pool: &sqlx::PgPool, req_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.tools (request_id, ord, tool_name)
         VALUES ($1, 0, 'Bash')",
    )
    .bind(req_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         SELECT $1, $2, 0, 'Read', '{\"file_path\":\"/x\"}'::jsonb, r.session_id
         FROM requests.requests r WHERE r.id = $2",
    )
    .bind(format!("tu-{}", uuid::Uuid::new_v4()))
    .bind(req_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_decodes_full_row_with_composition_and_tools() -> anyhow::Result<()> {
    let Some((pool, _db)) = isolated_pool().await? else {
        return Ok(());
    };

    let project_path = unique("fbid-proj");
    let project_id = seed_project(&pool, &project_path).await?;
    let session_uuid = uuid::Uuid::new_v4();
    let session_id = session_uuid.to_string();
    let session_fk = seed_session(&pool, session_uuid, project_id).await?;
    let req_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: Some("claude-opus-4-8"),
            turn_origin: "primary",
            created_at: Utc::now(),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let (rsp_id, comp_id) = seed_composition_chain(&pool, req_id).await?;
    let (fr_first, fr_second) = seed_firing_rules(&pool, rsp_id).await?;
    seed_declared_and_invoked_tool(&pool, req_id).await?;

    let got = fetch_request_by_id(&pool, req_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("request must be found"))?;

    assert_eq!(got.id.as_u64(), req_id as u64);
    assert_eq!(
        got.session_id, session_id,
        "session string via sessions join"
    );
    assert_eq!(
        got.project.as_deref(),
        Some(project_path.as_str()),
        "project path via sessions -> projects join"
    );
    assert_eq!(got.model.as_deref(), Some("claude-opus-4-8"));
    assert_eq!(
        got.system_prompt.composition_id,
        Some(comp_id),
        "composition bigint FK decoded from rsp.override_id"
    );
    assert!(got.system_prompt.composition_applied);
    assert!(got.system_prompt.filter_modified);
    assert_eq!(
        got.system_prompt.firing_rule_ids,
        vec![fr_first, fr_second],
        "firing rule bigint ids reassembled from the FK junction in fire \
         order (ord 0 then 1), proving array_agg ORDER BY j.ord despite \
         reversed insertion order"
    );
    assert_eq!(got.tool_names, vec!["Bash".to_string()], "DECLARED tool");
    assert_eq!(got.tool_calls.len(), 1, "one INVOKED tool");
    assert_eq!(got.tool_calls[0].name, "Read");
    assert!(
        got.tool_calls[0].input_json.contains("/x"),
        "tool input JSONB round-trips in full"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_request_by_id_unknown_returns_none() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let got = fetch_request_by_id(&pool, 999_999_999_999).await?;
    assert!(got.is_none(), "unknown id must map to Ok(None)");
    pool.close().await;
    Ok(())
}
