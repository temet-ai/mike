pub(crate) fn unit_vec(slot: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    v[slot] = 1.0;
    v
}

pub(crate) fn vector_to_text_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 12 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}

pub(crate) async fn seed_agent(
    pool: &sqlx::PgPool,
    name: &str,
) -> anyhow::Result<(i64, uuid::Uuid)> {
    let row: (i64, uuid::Uuid) = sqlx::query_as(
        "INSERT INTO agents.agents (name, source_id)
         SELECT $1, s.id FROM agents.sources s WHERE s.code = 'observed'
         RETURNING id, uuid",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub(crate) async fn seed_daily_lesson_with_embedding(
    pool: &sqlx::PgPool,
    project_id: i64,
    agent_db_id: Option<i64>,
    text: &str,
    embedding: &[f32],
) -> anyhow::Result<i64> {
    let literal = vector_to_text_literal(embedding);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons
            (project_id, agent_id, kind_id, text, stage_id, source_id,
             embedding, embedding_model, embedded_at)
         SELECT $1, $2, lk.id, $3, ls.id, lsrc.id, $4::vector, 'bge-small-en-v1.5', NOW()
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'convention' AND ls.code = 'daily' AND lsrc.code = 'operator'
         RETURNING id",
    )
    .bind(project_id)
    .bind(agent_db_id)
    .bind(text)
    .bind(&literal)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
