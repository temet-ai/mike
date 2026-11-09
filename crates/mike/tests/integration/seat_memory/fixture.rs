use serde_json::{Value, json};
use sqlx::PgPool;

pub(crate) struct Seat {
    pub(crate) session_id: i64,
    pub(crate) session_uuid: String,
}

pub(crate) struct LaunchSpec<'a> {
    pub(crate) run: Option<&'a str>,
    pub(crate) seat: &'a str,
    pub(crate) brief: &'a str,
    pub(crate) result: &'a str,
}

pub(crate) struct TurnSpec<'a> {
    pub(crate) opened_by: &'a str,
    pub(crate) kind: &'a str,
    pub(crate) session_class: &'a str,
    pub(crate) user_prompt: &'a str,
    pub(crate) final_text: &'a str,
}

pub(crate) async fn project(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name)
         VALUES ('/seat-memory/' || gen_random_uuid()::text, 'seat memory test')
         RETURNING id",
    )
    .fetch_one(pool)
    .await?)
}

pub(crate) async fn run(pool: &PgPool) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "INSERT INTO launch.runs (run_id, backend)
         VALUES ('seat-memory-' || gen_random_uuid()::text, 'herdr')
         RETURNING run_id",
    )
    .fetch_one(pool)
    .await?)
}

pub(crate) async fn session(pool: &PgPool, project_id: i64) -> anyhow::Result<Seat> {
    let (session_id, session_uuid): (i64, String) = sqlx::query_as(
        "INSERT INTO sessions.sessions (session_uuid, project_id)
         VALUES (gen_random_uuid(), $1)
         RETURNING id, session_uuid::text",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(Seat {
        session_id,
        session_uuid,
    })
}

pub(crate) async fn launch(
    pool: &PgPool,
    seat: &Seat,
    spec: &LaunchSpec<'_>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.launches
            (session_uuid, backend, folder, run_id, seat, brief_text, result, opened_at)
         VALUES ($1::text::uuid, 'herdr', '/work', $2, $3, $4, $5, now())",
    )
    .bind(&seat.session_uuid)
    .bind(spec.run)
    .bind(spec.seat)
    .bind(spec.brief)
    .bind(spec.result)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn template(pool: &PgPool, seat: &Seat) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.templates (name, session_uuid, folder, backend, claude_args)
         VALUES ('seat-memory-' || gen_random_uuid()::text, $1::text::uuid, '/work', 'herdr', '[]')",
    )
    .bind(&seat.session_uuid)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn event(
    pool: &PgPool,
    run: &str,
    session_uuid: Option<&str>,
    task: Option<&str>,
    kind: &str,
    payload: &Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.fleet_events (run_id, task, session_uuid, kind, payload)
         VALUES ($1, $2, $3::text::uuid, $4, $5)",
    )
    .bind(run)
    .bind(task)
    .bind(session_uuid)
    .bind(kind)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn verdict(
    pool: &PgPool,
    run: &str,
    task: &str,
    criterion: &str,
    passed: bool,
) -> anyhow::Result<()> {
    let payload = json!({"criterion": criterion, "passed": passed});
    event(pool, run, None, Some(task), "verdict_recorded", &payload).await
}

pub(crate) async fn report_note(pool: &PgPool, seat: &Seat, body: &str) -> anyhow::Result<String> {
    Ok(sqlx::query_scalar(
        "INSERT INTO workspace.session_notes (session_id, title, body)
         VALUES ($1, 'report', $2)
         RETURNING note_uuid::text",
    )
    .bind(seat.session_id)
    .bind(body)
    .fetch_one(pool)
    .await?)
}

pub(crate) async fn thread_entry(
    pool: &PgPool,
    project_id: i64,
    seat: &Seat,
    body: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "WITH t AS (
             INSERT INTO work.threads (project_id, slug, title)
             VALUES ($1, 'seat-memory-' || gen_random_uuid()::text, 'seat memory test')
             RETURNING id
         )
         INSERT INTO work.thread_log (thread_id, ord, session_uuid, summary, body)
         SELECT id, 1, $2::text::uuid, 'summary', $3 FROM t",
    )
    .bind(project_id)
    .bind(&seat.session_uuid)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn turn(
    pool: &PgPool,
    project_id: i64,
    seat: &Seat,
    spec: &TurnSpec<'_>,
) -> anyhow::Result<i64> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id
         )
         SELECT $1, $2, rm.id, '/v1/messages', false, 0, false,
                rs.id, '{}'::jsonb, '', $3, rtt.id, rto.id
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(seat.session_id)
    .bind(project_id)
    .bind(spec.user_prompt)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
         SELECT $1, $2, k.id FROM prompt_kinds.kinds k WHERE k.code = $3",
    )
    .bind(request_id)
    .bind(seat.session_id)
    .bind(spec.kind)
    .execute(pool)
    .await?;
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
            (session_id, project_id, start_request_id, end_request_id,
             user_prompt, final_assistant_text, opened_by, closed_at)
         VALUES ($1, $2, $3, $3, $4, $5, $6, now())
         RETURNING id",
    )
    .bind(seat.session_id)
    .bind(project_id)
    .bind(request_id)
    .bind(spec.user_prompt)
    .bind(spec.final_text)
    .bind(spec.opened_by)
    .fetch_one(pool)
    .await?;
    facts(pool, seat, turn_id, request_id, spec.session_class).await?;
    Ok(turn_id)
}

async fn facts(
    pool: &PgPool,
    seat: &Seat,
    turn_id: i64,
    request_id: i64,
    session_class: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.turn_facts
            (operator_turn_id, session_id, builder_version, built_at, max_request_id,
             prompt_kind, prompt_kind_at_close, next_state, next_state_at_close,
             text_chars, text_cut_at, aborted, interrupted, close_cause, ended_on,
             ended_on_version, session_class, session_class_rule)
         VALUES ($1, $2, 'test', now(), $3, 'genuine', 'genuine', 'none_session_settled',
                 'none_session_settled', 0, 0, false, false, 'next_prompt', 'none',
                 'test', $4, 'test')",
    )
    .bind(turn_id)
    .bind(seat.session_id)
    .bind(request_id)
    .bind(session_class)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn mail_turn(
    pool: &PgPool,
    project_id: i64,
    from: &Seat,
    to: &Seat,
    body: &str,
) -> anyhow::Result<String> {
    let message_id: String = sqlx::query_scalar(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
         VALUES (gen_random_uuid(), $1, $2, $3)
         RETURNING message_uuid::text",
    )
    .bind(to.session_id)
    .bind(from.session_id)
    .bind(body)
    .fetch_one(pool)
    .await?;
    let prompt = format!(
        "<channel source=\"mike\" from_session=\"{}\" kind=\"mailbox_message\" message_id=\"{message_id}\">\n{body}\n</channel>",
        from.session_uuid
    );
    let spec = TurnSpec {
        opened_by: "machine",
        kind: "mailbox-delivery",
        session_class: "fleet_seat",
        user_prompt: &prompt,
        final_text: "",
    };
    turn(pool, project_id, to, &spec).await?;
    Ok(message_id)
}
