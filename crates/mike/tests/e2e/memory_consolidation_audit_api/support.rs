use sqlx::PgPool;
use uuid::Uuid;

pub(super) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, $2, $2
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

pub(super) async fn insert_lesson(
    pool: &PgPool,
    project_id: i64,
    text: &str,
    stage: &str,
) -> anyhow::Result<(i64, Uuid)> {
    let row: (i64, Uuid) = sqlx::query_as(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, $2, ls.id, lsrc.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'convention' AND ls.code = $3 AND lsrc.code = 'operator'
         RETURNING id, uuid",
    )
    .bind(project_id)
    .bind(text)
    .bind(stage)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub(super) async fn archive_lesson_as_duplicate(
    pool: &PgPool,
    lesson_id: i64,
    duplicate_of_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons l
            SET stage_id           = archived_s.id,
                archived_at        = NOW(),
                archived_reason_id = ar.id,
                duplicate_of_id    = $2
           FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived' AND ar.code = 'duplicate' AND l.id = $1",
    )
    .bind(lesson_id)
    .bind(duplicate_of_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn start_consolidation_run(
    pool: &PgPool,
    kind: &str,
    trigger: &str,
    status: &str,
    started_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at, finished_at)
         SELECT k.id, t.id, s.id, $4, CASE WHEN $3 = 'running' THEN NULL ELSE $4 END
           FROM memory.consolidation_run_kinds k, memory.consolidation_triggers t, memory.consolidation_run_statuses s
          WHERE k.code = $1 AND t.code = $2 AND s.code = $3
         RETURNING id",
    )
    .bind(kind)
    .bind(trigger)
    .bind(status)
    .bind(started_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) struct ActionFixture<'a> {
    pub(super) run_id: i64,
    pub(super) lesson_id: i64,
    pub(super) lesson_uuid: Uuid,
    pub(super) project_id: Option<i64>,
    pub(super) session_uuid: Option<&'a str>,
    pub(super) files_touched: Option<&'a [&'a str]>,
    pub(super) lesson_text: &'a str,
    pub(super) action_kind: &'a str,
    pub(super) outcome: &'a str,
    pub(super) error: Option<&'a str>,
    pub(super) distance: Option<f32>,
    pub(super) threshold: Option<f32>,
    pub(super) counterpart_lesson_id: Option<i64>,
    pub(super) counterpart_uuid: Option<Uuid>,
    pub(super) counterpart_text: Option<&'a str>,
    pub(super) runners_up: serde_json::Value,
    pub(super) created_at: chrono::DateTime<chrono::Utc>,
}

pub(super) async fn insert_action(pool: &PgPool, f: ActionFixture<'_>) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_actions
            (run_id, lesson_id, lesson_uuid, project_id, session_id, session_uuid, files_touched,
             lesson_text, action_kind_id, outcome_id, error,
             distance, threshold, counterpart_lesson_id, counterpart_uuid, counterpart_text,
             runners_up, created_at)
         SELECT $1, $2, $3, $4, sess.id, $5::text::uuid, $6,
                $7, ak.id, oc.id, $10,
                $11, $12, $13, $14, $15,
                $16, $17
           FROM memory.consolidation_action_kinds ak
          CROSS JOIN memory.consolidation_outcomes oc
           LEFT JOIN sessions.sessions sess ON sess.session_uuid = $5::text::uuid
          WHERE ak.code = $8 AND oc.code = $9
         RETURNING id",
    )
    .bind(f.run_id)
    .bind(f.lesson_id)
    .bind(f.lesson_uuid)
    .bind(f.project_id)
    .bind(f.session_uuid)
    .bind(f.files_touched)
    .bind(f.lesson_text)
    .bind(f.action_kind)
    .bind(f.outcome)
    .bind(f.error)
    .bind(f.distance)
    .bind(f.threshold)
    .bind(f.counterpart_lesson_id)
    .bind(f.counterpart_uuid)
    .bind(f.counterpart_text)
    .bind(&f.runners_up)
    .bind(f.created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
