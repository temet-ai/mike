use super::*;

pub(super) fn activity(
    page: &FlowPage,
    table: &str,
    id: i64,
    request: Option<i64>,
    phase: &str,
    time: DateTime<Utc>,
) -> FlowActivity {
    build::activity(
        build::source(table, id, page.session_id, request),
        phase,
        format!("mike:{}:{table}", page.session_id),
        page.coverage.component.clone(),
        time,
    )
}

pub(super) fn push(page: &mut FlowPage, activity: FlowActivity) {
    page.actors.push(build::actor(
        page.session_id,
        FlowActorKind::Mike,
        activity.actor_id.clone(),
        "Mike".into(),
        Some(activity.source.clone()),
    ));
    if let Some(request) = activity.source.request_id {
        page.relationships.push(build::relationship(
            &activity,
            FlowRelationshipKind::ActivityRequest,
            "request",
            Some(activity.id.clone()),
            Some(format!("requests.requests:{request}:started")),
        ));
    }
    page.activities.push(activity);
}

pub(super) fn outcome(code: &str) -> FlowOutcome {
    match code {
        "applied" => FlowOutcome::Applied,
        "unchanged" => FlowOutcome::Unchanged,
        "skipped" | "refused_ceiling" | "refused_rate" | "refused_spend" => FlowOutcome::Skipped,
        "failed_closed" => FlowOutcome::FailedClosed,
        "ok" | "success" | "completed" => FlowOutcome::Success,
        "failure" | "failed" | "http_error" | "transport_error" | "timeout" | "truncated" => {
            FlowOutcome::Error
        }
        "resolved_pass" | "resolved_correction" => FlowOutcome::Resolved,
        _ => FlowOutcome::Unknown,
    }
}
