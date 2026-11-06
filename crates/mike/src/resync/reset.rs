use sqlx::PgPool;

use crate::store::persistence::migrations;

use super::ResyncError;

pub async fn reset_local(pool: &PgPool) -> Result<(), ResyncError> {
    sqlx::query!(
        "DROP SCHEMA IF EXISTS agents, claude_proxy, config, eval, jobs, launch, llm, mailbox, \
         memory, messages, mike, overrides, projects, prompt_kinds, remote_control, requests, \
         responses, rules, runtime, session_judges, sessions, steering, sync, system_prompts, \
         tools, work, workspace CASCADE"
    )
    .execute(pool)
    .await?;
    sqlx::query!("DROP TABLE IF EXISTS _sqlx_migrations")
        .execute(pool)
        .await?;
    migrations::run(pool).await?;
    Ok(())
}
