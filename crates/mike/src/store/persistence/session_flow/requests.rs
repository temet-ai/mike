use super::*;

struct Row {
    id: i64,
    created_at: DateTime<Utc>,
    persisted_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    status: String,
    origin: String,
    model: Option<String>,
    claude_code_agent_id: Option<String>,
    agent_instance_id: Option<i64>,
    operator_turn_id: Option<i64>,
    body_available: bool,
    response_id: Option<i64>,
    upstream_message_id: Option<String>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT r.id, r.created_at, r.persisted_at, r.completed_at, s.code AS status,
                  o.code AS origin, CASE WHEN octet_length(r.model) <= 128 THEN r.model END AS "model?",
                  r.claude_code_agent_id, r.agent_instance_id, r.operator_turn_id,
                  (r.request_body IS NOT NULL OR r.request_envelope IS NOT NULL) AS "body_available!",
                  resp.id AS "response_id?",
                  CASE WHEN octet_length(resp.upstream_message_id) <= 128
                       THEN resp.upstream_message_id END AS "upstream_message_id?"
             FROM requests.requests r JOIN requests.statuses s ON s.id = r.status_id
             JOIN requests.turn_origins o ON o.id = r.turn_origin_id
             LEFT JOIN responses.responses resp ON resp.request_id = r.id
            WHERE r.id = ANY($1) ORDER BY r.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        append(page, row);
    }
    Ok(())
}

fn append(page: &mut FlowPage, row: Row) {
    let source = build::source("requests.requests", row.id, page.session_id, Some(row.id));
    let actor = build::request_actor(
        page.session_id,
        row.id,
        row.claude_code_agent_id,
        row.agent_instance_id,
        &row.origin,
        &source,
    );
    let mut started = build::activity(
        source.clone(),
        "started",
        actor.id.clone(),
        FlowComponent::Requests,
        row.created_at,
    );
    started.recorded_at = row.persisted_at;
    started.ended_at = row.completed_at;
    started.time_semantics = FlowTimeSemantics::CaptureInterval;
    started.lifecycle = FlowLifecycle::Started;
    started.metadata.model = build::vocabulary(row.model);
    started.metadata.heuristic_instance_id = row.agent_instance_id;
    started.metadata.turn_id = row.operator_turn_id;
    started.metadata.body_available = Some(row.body_available);
    page.activities.push(started);
    if let Some(end) = row.completed_at {
        let mut completed = build::activity(
            source,
            "completed",
            actor.id.clone(),
            FlowComponent::Requests,
            end,
        );
        completed.lifecycle = FlowLifecycle::Completed;
        completed.outcome = Some(match row.status.as_str() {
            "complete" => FlowOutcome::Success,
            "error" => FlowOutcome::Error,
            _ => FlowOutcome::Unknown,
        });
        page.activities.push(completed);
        push_upstream(page, row.id, row.response_id, row.upstream_message_id);
    }
    page.actors.push(actor);
}

fn push_upstream(
    page: &mut FlowPage,
    request_id: i64,
    response_id: Option<i64>,
    upstream_message_id: Option<String>,
) {
    let (Some(response_id), Some(upstream_message_id)) = (response_id, upstream_message_id) else {
        return;
    };
    let mut upstream = build::actor(
        page.session_id,
        FlowActorKind::Upstream,
        format!("upstream:{}", page.session_id),
        "Model API".into(),
        Some(build::source(
            "responses.responses",
            response_id,
            page.session_id,
            Some(request_id),
        )),
    );
    upstream.producer_id = Some(upstream_message_id);
    page.actors.push(upstream);
}
