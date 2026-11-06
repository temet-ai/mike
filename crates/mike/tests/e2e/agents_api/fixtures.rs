use chrono::Utc;
use sqlx::PgPool;

pub(super) async fn seed_agent(pool: &sqlx::PgPool, name: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar("INSERT INTO agents.agents (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

pub(super) async fn seed_agent_tool_use(
    pool: &PgPool,
    tool_id: &str,
    request_id: i64,
    ord: i32,
    created_at: chrono::DateTime<Utc>,
    prompt: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, created_at, session_id)
         VALUES ($1, $2, $3, 'Agent', jsonb_build_object('prompt', $4::text), $5,
                 (SELECT session_id FROM requests.requests WHERE id = $2))
         RETURNING id",
    )
    .bind(tool_id)
    .bind(request_id)
    .bind(ord)
    .bind(prompt)
    .bind(created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_dispatch_for_agent(
    pool: &PgPool,
    tool_use_pk: i64,
    agent_id: i64,
) -> anyhow::Result<i64> {
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(tool_use_pk)
    .bind(agent_id)
    .fetch_one(pool)
    .await?;
    Ok(dispatch_id)
}

pub(super) async fn seed_instance(
    pool: &PgPool,
    session_pk: i64,
    dispatch_id: i64,
    agent_id: i64,
    persona: Option<&str>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.instances (session_id, dispatch_id, agent_id, subagent_persona)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(session_pk)
    .bind(dispatch_id)
    .bind(agent_id)
    .bind(persona)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) struct SessionPromptSeed<'a> {
    pub session_pk: i64,
    pub request_id: i64,
    pub agent_id: i64,
    pub lane: &'a str,
    pub content_sha256: &'a str,
    pub created_at: chrono::DateTime<Utc>,
}

pub(super) async fn seed_session_prompt(
    pool: &PgPool,
    seed: SessionPromptSeed<'_>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, agent_id, blocks, content_sha256, created_at)
         VALUES ($1, $2, $3, $4, '[]'::jsonb, $5, $6)
         RETURNING id",
    )
    .bind(seed.session_pk)
    .bind(seed.request_id)
    .bind(seed.lane)
    .bind(seed.agent_id)
    .bind(seed.content_sha256)
    .bind(seed.created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_alias(
    pool: &PgPool,
    agent_id: i64,
    alias: &str,
    kind: &str,
) -> anyhow::Result<()> {
    let query = match kind {
        "persona" => {
            "UPDATE agents.agents
                SET persona_aliases = ARRAY_APPEND(persona_aliases, $2)
              WHERE id = $1"
        }
        "name_variant" => {
            "UPDATE agents.agents
                SET name_variants = ARRAY_APPEND(name_variants, $2)
              WHERE id = $1"
        }
        other => anyhow::bail!("unsupported agent alias kind in test fixture: {other}"),
    };
    sqlx::query(query)
        .bind(agent_id)
        .bind(alias)
        .execute(pool)
        .await?;
    Ok(())
}
