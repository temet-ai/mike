use chrono::{Duration, Utc};
use uuid::Uuid;

use super::keeper_earlier::EarlierQuery;
use crate::store::operator_turns::vector_to_text_literal;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns, seed_two_sessions,
};

fn axis(first: f32, second: f32) -> String {
    let mut v = vec![0.0_f32; 768];
    v[0] = first;
    v[1] = second;
    vector_to_text_literal(&v)
}

async fn session_with_chunk(
    handle: &PersistHandle,
    project: i64,
    vector: &str,
    age: Duration,
) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    seed_two_sessions(
        handle.pool(),
        &session.to_string(),
        &Uuid::new_v4().to_string(),
        project,
    )
    .await?;
    seed_session_turns(handle.pool(), &session.to_string(), project, 1).await?;
    sqlx::query(
        "WITH t AS (
            UPDATE memory.operator_turns t SET opened_by = 'operator', created_at = now() - $3::interval
              FROM sessions.sessions s
             WHERE s.id = t.session_id AND s.session_uuid = $1
         RETURNING t.id)
         INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT t.id, 0, r.id, 'chunk', $2::public.vector, 'embeddinggemma'
           FROM t, memory.chunk_roles r WHERE r.code = 'prompt'",
    )
    .bind(session)
    .bind(vector)
    .bind(format!("{} seconds", age.num_seconds()))
    .execute(handle.pool())
    .await?;
    Ok(session)
}

#[tokio::test]
async fn earlier_sessions_in_the_window_come_best_first_without_the_own_session()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let path = format!("/work/earlier-{}", Uuid::new_v4());
    let project = seed_project(handle.pool(), &path).await?;
    let own = session_with_chunk(&handle, project, &axis(1.0, 0.0), Duration::hours(2)).await?;
    let close = session_with_chunk(&handle, project, &axis(1.0, 0.0), Duration::hours(3)).await?;
    let partial = session_with_chunk(&handle, project, &axis(0.6, 0.8), Duration::hours(4)).await?;
    let unrelated =
        session_with_chunk(&handle, project, &axis(0.0, 1.0), Duration::hours(5)).await?;
    let too_old = session_with_chunk(&handle, project, &axis(1.0, 0.0), Duration::days(40)).await?;
    let now = Utc::now();

    let rows = handle
        .earlier_sessions(&EarlierQuery {
            project_paths: &[path],
            vector_literal: &axis(1.0, 0.0),
            model: "embeddinggemma",
            from: now - Duration::days(30),
            to: now,
            exclude_session: own,
            min_similarity: 0.35,
            limit: 3,
        })
        .await?;

    let sessions: Vec<Uuid> = rows.iter().map(|r| r.session_uuid).collect();
    assert_eq!(sessions, [close, partial]);
    assert!(!sessions.contains(&unrelated) && !sessions.contains(&too_old));
    assert!((rows[1].similarity - 0.6).abs() < 1e-4);
    db.cleanup().await?;
    Ok(())
}
