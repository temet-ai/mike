use mike::store::persistence::session_flow::{
    FlowActorKind, FlowComponent, FlowOutcome, FlowProvenance, FlowReadError, FlowRelationshipKind,
    FlowRowId,
};
use mike::store::persistence::web_repo::WebPersist as _;

use super::support::{Fixture, ids, no_raw};

#[tokio::test]
async fn request_workers_unknown_ownership_and_late_outcomes_stay_separate() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let a = f.request(Some("worker-a"), "subagent").await?;
    let b = f.request(Some("worker-b"), "subagent").await?;
    let unknown = f.request(None, "subagent").await?;
    sqlx::query(
        "UPDATE requests.requests SET subagent_persona = 'same-persona' WHERE session_id = $1",
    )
    .bind(f.id)
    .execute(f.handle.pool())
    .await?;
    sqlx::query("UPDATE requests.requests SET completed_at = '2026-09-10T12:00:05Z', status_id = 4 WHERE id = $1")
        .bind(a).execute(f.handle.pool()).await?;
    sqlx::query(
        "UPDATE requests.requests SET request_body = NULL, request_envelope = NULL WHERE id = $1",
    )
    .bind(b)
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::Requests, 10, None).await?;
    assert_eq!(page.actors.len(), 3);
    assert_eq!(
        page.actors
            .iter()
            .filter(|a| a.provenance == FlowProvenance::ProducerHeader)
            .count(),
        2
    );
    assert!(
        page.actors.iter().any(
            |a| a.provenance == FlowProvenance::Unknown && a.id.ends_with(&unknown.to_string())
        )
    );
    let began = page
        .activities
        .iter()
        .find(|v| v.source.row_id == FlowRowId::Bigint(a) && v.phase == "started")
        .ok_or_else(|| anyhow::anyhow!("start missing"))?;
    assert_eq!(began.outcome, None);
    assert!(began.ended_at.is_some());
    assert_ne!(
        began.recorded_at,
        began
            .occurred_at
            .ok_or_else(|| anyhow::anyhow!("capture occurrence missing"))?
    );
    assert!(
        page.activities
            .iter()
            .any(|v| v.source.row_id == FlowRowId::Bigint(a)
                && v.phase == "completed"
                && v.outcome == Some(FlowOutcome::Error))
    );
    assert!(page.activities.iter().any(
        |v| v.source.row_id == FlowRowId::Bigint(b) && v.metadata.body_available == Some(false)
    ));
    no_raw(&page);
    f.cleanup().await
}

#[tokio::test]
async fn eight_identical_tools_dispatches_and_deduped_results_page_by_source_id()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let emitter = f.request(None, "primary").await?;
    let carrier = f.request(None, "primary").await?;
    let agent: i64 = f.agent("same-persona").await?;
    let mut tools = vec![];
    let mut dispatches = vec![];
    for ord in 0..8_i32 {
        let id: i64 = sqlx::query_scalar("INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
            VALUES ($1, $2, $3, $4, 'Agent', '{\"prompt\":\"RAW_SENTINEL\"}', '2026-09-10T12:00:02Z') RETURNING id")
            .bind(emitter).bind(f.id).bind(ord).bind(format!("tool-{ord}")).fetch_one(f.handle.pool()).await?;
        tools.push(id);
        dispatches.push(sqlx::query_scalar::<_, i64>("INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id")
            .bind(id).bind(agent).fetch_one(f.handle.pool()).await?);
    }
    let first = f.page(FlowComponent::Tools, 3, None).await?;
    let second = f
        .page(FlowComponent::Tools, 5, first.next_cursor.as_ref())
        .await?;
    assert_eq!([ids(&first), ids(&second)].concat(), tools);
    assert_eq!(
        first
            .activities
            .iter()
            .map(|a| a.metadata.ordinal)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2)]
    );
    let dispatch = f.page(FlowComponent::Dispatches, 10, None).await?;
    assert_eq!(ids(&dispatch), dispatches);
    assert!(
        dispatch
            .actors
            .iter()
            .all(|a| a.kind == FlowActorKind::UnresolvedDispatch)
    );
    let results = seed_results(&f, carrier, &tools).await?;
    let result_page = f.page(FlowComponent::Results, 10, None).await?;
    assert_eq!(ids(&result_page), results);
    assert_eq!(
        result_page
            .activities
            .iter()
            .map(|a| a.outcome.clone())
            .collect::<Vec<_>>(),
        vec![Some(FlowOutcome::Success), Some(FlowOutcome::Error)]
    );
    assert!(
        result_page
            .activities
            .iter()
            .all(|a| a.source.request_id == Some(carrier))
    );
    assert!(
        result_page
            .relationships
            .iter()
            .any(|r| r.kind == FlowRelationshipKind::ResultCarrier
                && r.to_id == Some(format!("requests.requests:{carrier}:started")))
    );
    for page in [&first, &dispatch, &result_page] {
        no_raw(page);
    }
    f.cleanup().await
}

async fn seed_results(f: &Fixture, carrier: i64, tools: &[i64]) -> anyhow::Result<Vec<i64>> {
    let mut ids = vec![];
    for (ord, tool) in tools.iter().take(2).enumerate() {
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO tools.results (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes)
            VALUES ($1, $2, $3, $4, $5, '\"RAW_SENTINEL\"', 'fingerprint', 12) ON CONFLICT (tool_use_id) DO NOTHING RETURNING id")
            .bind(carrier).bind(f.id).bind(tool).bind(ord as i32).bind(ord == 1).fetch_one(f.handle.pool()).await?;
        ids.push(id);
        let duplicate = sqlx::query_scalar::<_, i64>("INSERT INTO tools.results (request_id, session_id, tool_use_id, ord, is_error, content, content_sha256, content_bytes)
            VALUES ($1, $2, $3, $4, false, '\"RAW_SENTINEL\"', 'fingerprint', 12) ON CONFLICT (tool_use_id) DO NOTHING RETURNING id")
            .bind(carrier).bind(f.id).bind(tool).bind(ord as i32).fetch_optional(f.handle.pool()).await?;
        assert!(duplicate.is_none());
    }
    Ok(ids)
}

#[tokio::test]
async fn heuristic_instance_id_without_producer_header_yields_heuristic_provenance()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let bound = f.request(None, "subagent").await?;
    let unbound = f.request(None, "subagent").await?;
    let emitter = f.request(None, "primary").await?;
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 0, 'heuristic-probe', 'Agent', '{}', '2026-09-10T12:00:02Z') RETURNING id",
    )
    .bind(emitter)
    .bind(f.id)
    .fetch_one(f.handle.pool())
    .await?;
    let agent_id: i64 = f.agent("reviewer").await?;
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(tool_use_id)
    .bind(agent_id)
    .fetch_one(f.handle.pool())
    .await?;
    let instance_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.instances (session_id, dispatch_id, subagent_persona)
         VALUES ($1, $2, 'reviewer') RETURNING id",
    )
    .bind(f.id)
    .bind(dispatch_id)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query("UPDATE requests.requests SET agent_instance_id = $2 WHERE id = $1")
        .bind(bound)
        .bind(instance_id)
        .execute(f.handle.pool())
        .await?;
    let page = f.page(FlowComponent::Requests, 10, None).await?;
    let bound_actor = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::Worker && a.id.ends_with(&bound.to_string()))
        .ok_or_else(|| anyhow::anyhow!("bound actor missing"))?;
    assert_eq!(bound_actor.provenance, FlowProvenance::Heuristic);
    assert_eq!(bound_actor.parent_actor_id, None);
    let unbound_actor = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::Worker && a.id.ends_with(&unbound.to_string()))
        .ok_or_else(|| anyhow::anyhow!("unbound actor missing"))?;
    assert_eq!(unbound_actor.provenance, FlowProvenance::Unknown);
    f.cleanup().await
}

#[tokio::test]
async fn dispatch_with_agents_instances_row_gets_explicit_correlation_and_parent_edge()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let emitter = f.request(None, "primary").await?;
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 0, 'correlation-probe', 'Agent', '{}', '2026-09-10T12:00:02Z') RETURNING id",
    )
    .bind(emitter)
    .bind(f.id)
    .fetch_one(f.handle.pool())
    .await?;
    let agent_id: i64 = f.agent("reviewer").await?;
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(tool_use_id)
    .bind(agent_id)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO agents.instances (session_id, dispatch_id, subagent_persona)
         VALUES ($1, $2, 'reviewer')",
    )
    .bind(f.id)
    .bind(dispatch_id)
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::Dispatches, 10, None).await?;
    let dispatch_actor = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::UnresolvedDispatch)
        .ok_or_else(|| anyhow::anyhow!("dispatch actor missing"))?
        .id
        .clone();
    let worker = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::Worker)
        .ok_or_else(|| anyhow::anyhow!("explicitly-correlated worker actor missing"))?;
    assert_eq!(worker.provenance, FlowProvenance::ExplicitCorrelation);
    assert_eq!(worker.label, "reviewer");
    assert_eq!(worker.parent_actor_id, Some(dispatch_actor.clone()));
    assert!(
        page.relationships
            .iter()
            .any(|r| r.kind == FlowRelationshipKind::ExplicitParent
                && r.to_id == Some(worker.id.clone()))
    );
    f.cleanup().await
}

#[tokio::test]
async fn completed_request_with_response_row_gets_upstream_actor() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let id = f.request(None, "primary").await?;
    sqlx::query(
        "UPDATE requests.requests SET completed_at = '2026-09-10T12:00:05Z', status_id = 3 WHERE id = $1",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, upstream_message_id, completed_at)
         VALUES ($1, (SELECT id FROM responses.statuses LIMIT 1), 'msg_test_upstream', '2026-09-10T12:00:05Z')",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::Requests, 10, None).await?;
    let upstream = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::Upstream)
        .ok_or_else(|| anyhow::anyhow!("expected an Upstream actor"))?;
    assert_eq!(upstream.producer_id.as_deref(), Some("msg_test_upstream"));
    assert_eq!(upstream.provenance, FlowProvenance::SourceRow);
    f.cleanup().await
}

#[tokio::test]
async fn real_sidecar_codes_remain_observations_and_database_errors_stay_errors()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    for origin in ["title_sidecar", "advisor_sidecar", "quota_probe"] {
        f.request(None, origin).await?;
    }
    let page = f.page(FlowComponent::Requests, 10, None).await?;
    assert_eq!(page.actors.len(), 3);
    assert!(
        page.actors
            .iter()
            .all(|a| a.kind == FlowActorKind::Sidecar && a.parent_actor_id.is_none())
    );
    assert!(page.relationships.is_empty());
    f.handle.pool().close().await;
    assert!(matches!(
        f.handle
            .web()
            .fetch_session_flow(f.session, FlowComponent::Requests, 10, None)
            .await,
        Err(FlowReadError::Database(_))
    ));
    f.cleanup().await
}

#[tokio::test]
async fn all_eleven_turn_origins_map_to_expected_actor_kind() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let cases: [(&str, FlowActorKind); 11] = [
        ("primary", FlowActorKind::Session),
        ("primary_first", FlowActorKind::Session),
        ("subagent", FlowActorKind::Worker),
        ("workflow_subagent", FlowActorKind::Worker),
        ("title_sidecar", FlowActorKind::Sidecar),
        ("advisor_sidecar", FlowActorKind::Sidecar),
        ("quota_probe", FlowActorKind::Sidecar),
        ("monitor_sidecar", FlowActorKind::Sidecar),
        ("count_tokens_sidecar", FlowActorKind::Sidecar),
        ("web_fetch_sidecar", FlowActorKind::Sidecar),
        ("next_input_suggestion_sidecar", FlowActorKind::Sidecar),
    ];
    let mut request_ids = Vec::with_capacity(cases.len());
    for (origin, _) in &cases {
        request_ids.push(f.request(None, origin).await?);
    }
    let page = f.page(FlowComponent::Requests, 20, None).await?;
    for ((origin, expected_kind), request_id) in cases.iter().zip(request_ids.iter()) {
        let started = page
            .activities
            .iter()
            .find(|a| a.source.row_id == FlowRowId::Bigint(*request_id) && a.phase == "started")
            .ok_or_else(|| anyhow::anyhow!("missing start activity for {origin}"))?;
        let actor = page
            .actors
            .iter()
            .find(|a| a.id == started.actor_id)
            .ok_or_else(|| anyhow::anyhow!("missing actor for {origin}"))?;
        assert_eq!(
            &actor.kind, expected_kind,
            "origin {origin} mapped to the wrong actor kind"
        );
    }
    f.cleanup().await
}

#[tokio::test]
async fn dispatch_activity_carries_sanitized_agent_name_and_description() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let emitter = f.request(None, "primary").await?;
    let agent_id: i64 = f.agent("code-reviewer").await?;
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 3, 'label-probe', 'Agent', $3, '2026-09-10T12:00:02Z') RETURNING id",
    )
    .bind(emitter)
    .bind(f.id)
    .bind(serde_json::json!({
        "description": "  Review   the auth\tmodule for security issues  ",
        "prompt": "RAW_SENTINEL",
        "subagent_type": "code-reviewer",
    }))
    .fetch_one(f.handle.pool())
    .await?;
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(tool_use_id)
    .bind(agent_id)
    .fetch_one(f.handle.pool())
    .await?;

    let oversized_use: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 4, 'label-probe-oversized', 'Agent', $3, '2026-09-10T12:00:03Z') RETURNING id",
    )
    .bind(emitter)
    .bind(f.id)
    .bind(serde_json::json!({ "description": "x".repeat(129) }))
    .fetch_one(f.handle.pool())
    .await?;
    let oversized_dispatch: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(oversized_use)
    .bind(agent_id)
    .fetch_one(f.handle.pool())
    .await?;

    let page = f.page(FlowComponent::Dispatches, 10, None).await?;
    let activity = page
        .activities
        .iter()
        .find(|a| a.source.row_id == FlowRowId::Bigint(dispatch_id))
        .ok_or_else(|| anyhow::anyhow!("dispatch activity missing"))?;
    assert_eq!(
        activity.metadata.agent_name.as_deref(),
        Some("code-reviewer")
    );
    assert_eq!(
        activity.metadata.dispatch_description.as_deref(),
        Some("Review the auth module for security issues")
    );

    let oversized_activity = page
        .activities
        .iter()
        .find(|a| a.source.row_id == FlowRowId::Bigint(oversized_dispatch))
        .ok_or_else(|| anyhow::anyhow!("oversized dispatch activity missing"))?;
    assert_eq!(
        oversized_activity.metadata.dispatch_description.as_deref(),
        Some("Agent 4")
    );
    f.cleanup().await
}

#[tokio::test]
async fn filter_stage_with_firing_rules_reports_firing_rule_count() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let id = f.request(None, "primary_first").await?;
    sqlx::query("UPDATE requests.requests SET transformation_manifest = $2 WHERE id = $1")
        .bind(id)
        .bind(serde_json::json!({
            "schema_version": 1,
            "stages": [
                {
                    "ordinal": 1,
                    "stage": "filter",
                    "outcome": "applied",
                    "resolved_parameters": { "firing_rule_ids": [7, 12, 19] },
                },
                {
                    "ordinal": 2,
                    "stage": "lazy_sections",
                    "outcome": "unchanged",
                    "resolved_parameters": { "rule_ids": [] },
                },
            ],
        }))
        .execute(f.handle.pool())
        .await?;
    let page = f.page(FlowComponent::ManifestStages, 10, None).await?;
    let filter_stage = page
        .activities
        .iter()
        .find(|a| a.metadata.stage.as_deref() == Some("filter"))
        .ok_or_else(|| anyhow::anyhow!("filter stage missing"))?;
    assert_eq!(filter_stage.metadata.firing_rule_count, Some(3));
    let other_stage = page
        .activities
        .iter()
        .find(|a| a.metadata.stage.as_deref() == Some("lazy_sections"))
        .ok_or_else(|| anyhow::anyhow!("lazy_sections stage missing"))?;
    assert_eq!(other_stage.metadata.firing_rule_count, None);
    f.cleanup().await
}
