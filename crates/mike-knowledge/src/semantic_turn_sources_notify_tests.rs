use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns,
};

const SENTINEL: &str = r#"{"table":"sentinel"}"#;

async fn turns(pool: &PgPool, count: i64) -> anyhow::Result<Vec<(i64, i64, i64)>> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let project = seed_project(pool, &format!("/semantic-turn-sources/{uuid}")).await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())",
    )
    .bind(&uuid)
    .bind(project)
    .execute(pool)
    .await?;
    seed_session_turns(pool, &uuid, project, count).await?;
    Ok(sqlx::query_as(
        "SELECT t.id, t.session_id, t.start_request_id FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid ORDER BY t.id",
    )
    .bind(&uuid)
    .fetch_all(pool)
    .await?)
}

async fn facts(
    pool: &PgPool,
    turns: &[(i64, i64, i64)],
    prompt_kind: &str,
    session_class: &str,
) -> anyhow::Result<()> {
    let ids: Vec<i64> = turns.iter().map(|t| t.0).collect();
    let sessions: Vec<i64> = turns.iter().map(|t| t.1).collect();
    let requests: Vec<i64> = turns.iter().map(|t| t.2).collect();
    sqlx::query(
        "INSERT INTO memory.turn_facts
            (operator_turn_id, session_id, builder_version, built_at, max_request_id,
             prompt_kind, prompt_kind_at_close, next_state, next_state_at_close,
             text_chars, text_cut_at, aborted, interrupted, close_cause, ended_on,
             ended_on_version, session_class, session_class_rule)
         SELECT t, s, 'test', now(), r, $4, $4, 'none_session_settled', 'none_session_settled',
                0, 0, false, false, 'next_prompt', 'none', 'test', $5, 'test'
           FROM unnest($1::bigint[], $2::bigint[], $3::bigint[]) AS x(t, s, r)",
    )
    .bind(&ids)
    .bind(&sessions)
    .bind(&requests)
    .bind(prompt_kind)
    .bind(session_class)
    .execute(pool)
    .await?;
    Ok(())
}

async fn update(pool: &PgPool, sql: &'static str, turn: i64) -> anyhow::Result<()> {
    sqlx::query(sql).bind(turn).execute(pool).await?;
    Ok(())
}

async fn notices_before_sentinel(listener: &mut PgListener) -> anyhow::Result<usize> {
    let mut seen = 0;
    loop {
        let notice = listener.recv().await?;
        if notice.payload() == SENTINEL {
            return Ok(seen);
        }
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "semantic_turn_sources" {
            seen += 1;
        }
    }
}

#[tokio::test]
async fn genuine_operator_facts_notify_once_per_statement_and_other_facts_stay_silent()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let seeded = turns(&pool, 4).await?;
    let [a, b, c, d] = seeded[..] else {
        anyhow::bail!("expected four turns, got {seeded:?}");
    };
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;

    facts(&pool, &[a], "empty", "operator").await?;
    facts(&pool, &[b], "genuine", "fleet_seat").await?;
    facts(&pool, &[c, d], "genuine", "operator").await?;
    update(
        &pool,
        "UPDATE memory.turn_facts SET prompt_kind = 'genuine', builder_version = 'again'
          WHERE operator_turn_id = $1",
        c.0,
    )
    .await?;
    update(
        &pool,
        "UPDATE memory.turn_facts SET prompt_kind = 'genuine' WHERE operator_turn_id = $1",
        a.0,
    )
    .await?;
    sqlx::query("SELECT pg_notify('proxy.row_changed', $1)")
        .bind(SENTINEL)
        .execute(&pool)
        .await?;

    assert_eq!(
        notices_before_sentinel(&mut listener).await?,
        2,
        "one notice for the statement that wrote two genuine operator facts, one for the reclassified turn"
    );
    pool.close().await;
    db.cleanup().await
}
