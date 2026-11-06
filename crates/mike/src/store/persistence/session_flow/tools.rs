use super::*;

struct ToolRow {
    ord: i32,
    id: i64,
    request_id: i64,
    created_at: DateTime<Utc>,
    wire_tool_use_id: String,
    tool_name: Option<String>,
}

pub(super) async fn read_uses(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        ToolRow,
        r#"SELECT id, ord, request_id, created_at, wire_tool_use_id,
                  CASE WHEN octet_length(tool_name) <= 128 THEN tool_name END AS "tool_name?"
             FROM tools.uses WHERE id = ANY($1) ORDER BY id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let source = build::source("tools.uses", row.id, page.session_id, Some(row.request_id));
        let name = build::vocabulary(row.tool_name);
        let actor = build::actor(
            page.session_id,
            FlowActorKind::ToolService,
            format!("tool:{}:{}", page.session_id, row.id),
            name.clone().unwrap_or_else(|| format!("Tool {}", row.id)),
            Some(source.clone()),
        );
        let mut activity = build::activity(
            source,
            "observed",
            actor.id.clone(),
            FlowComponent::Tools,
            row.created_at,
        );
        activity.metadata.tool_name = name;
        activity.metadata.ordinal = u32::try_from(row.ord).ok();
        activity.metadata.wire_tool_use_id = Some(row.wire_tool_use_id);
        page.relationships.push(build::relationship(
            &activity,
            FlowRelationshipKind::RequestTool,
            "emitter",
            Some(format!("requests.requests:{}:started", row.request_id)),
            Some(activity.id.clone()),
        ));
        page.actors.push(actor);
        page.activities.push(activity);
    }
    Ok(())
}

struct ResultRow {
    ord: i32,
    id: i64,
    tool_use_id: i64,
    request_id: i64,
    created_at: DateTime<Utc>,
    is_error: bool,
    emitter_session: Uuid,
    emitter_request: i64,
    tool_name: Option<String>,
    wire_tool_use_id: String,
}

pub(super) async fn read_results(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        ResultRow,
        r#"SELECT r.id, r.ord, r.tool_use_id, r.request_id, r.created_at, r.is_error,
                  s.session_uuid AS emitter_session, u.request_id AS emitter_request,
                  CASE WHEN octet_length(u.tool_name) <= 128 THEN u.tool_name END AS "tool_name?",
                  u.wire_tool_use_id
             FROM tools.results r JOIN tools.uses u ON u.id = r.tool_use_id
             JOIN sessions.sessions s ON s.id = u.session_id
            WHERE r.id = ANY($1) ORDER BY r.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let source = build::source(
            "tools.results",
            row.id,
            page.session_id,
            Some(row.request_id),
        );
        let name = build::vocabulary(row.tool_name);
        let actor = build::actor(
            row.emitter_session,
            FlowActorKind::ToolService,
            format!("tool:{}:{}", row.emitter_session, row.tool_use_id),
            name.clone()
                .unwrap_or_else(|| format!("Tool {}", row.tool_use_id)),
            Some(build::source(
                "tools.uses",
                row.tool_use_id,
                row.emitter_session,
                Some(row.emitter_request),
            )),
        );
        let mut activity = build::activity(
            source,
            "observed",
            actor.id.clone(),
            FlowComponent::Results,
            row.created_at,
        );
        activity.metadata.ordinal = u32::try_from(row.ord).ok();
        activity.metadata.tool_name = name;
        activity.metadata.wire_tool_use_id = Some(row.wire_tool_use_id);
        activity.outcome = Some(if row.is_error {
            FlowOutcome::Error
        } else {
            FlowOutcome::Success
        });
        page.relationships.push(build::relationship(
            &activity,
            FlowRelationshipKind::ResultCarrier,
            "carrier",
            Some(format!("tools.uses:{}:observed", row.tool_use_id)),
            Some(format!("requests.requests:{}:started", row.request_id)),
        ));
        page.relationships.push(build::relationship(
            &activity,
            FlowRelationshipKind::ActivityRequest,
            "request",
            Some(activity.id.clone()),
            Some(format!("requests.requests:{}:started", row.request_id)),
        ));
        page.actors.push(actor);
        page.activities.push(activity);
    }
    Ok(())
}
