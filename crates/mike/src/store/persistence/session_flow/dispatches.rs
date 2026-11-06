use super::*;

struct Row {
    id: i64,
    tool_use_id: i64,
    request_id: i64,
    created_at: DateTime<Utc>,
    wire_tool_use_id: String,
    ord: i32,
    agent_name: String,
    description: Option<String>,
    instance_id: Option<i64>,
    persona: Option<String>,
    worker_producer_id: Option<String>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT d.id AS "id!", d.tool_use_id AS "tool_use_id!", u.request_id AS "request_id!",
                  u.created_at AS "created_at!", u.wire_tool_use_id AS "wire_tool_use_id!", u.ord AS "ord!",
                  ag.name AS "agent_name!", u.input->>'description' AS "description?",
                  i.id AS "instance_id?",
                  CASE WHEN octet_length(i.subagent_persona) <= 128
                       THEN i.subagent_persona END AS "persona?",
                  d.worker_producer_id AS "worker_producer_id?"
             FROM agents.dispatches d JOIN tools.uses u ON u.id = d.tool_use_id
             JOIN agents.agents ag ON ag.id = d.agent_id
             LEFT JOIN agents.instances i ON i.dispatch_id = d.id
            WHERE d.id = ANY($1) ORDER BY d.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let source = build::source(
            "agents.dispatches",
            row.id,
            page.session_id,
            Some(row.request_id),
        );
        let actor = build::actor(
            page.session_id,
            FlowActorKind::UnresolvedDispatch,
            format!("dispatch:{}:{}", page.session_id, row.id),
            format!("Dispatch {}", row.id),
            Some(source.clone()),
        );
        let mut activity = build::activity(
            source,
            "observed",
            actor.id.clone(),
            FlowComponent::Dispatches,
            row.created_at,
        );
        activity.metadata.wire_tool_use_id = Some(row.wire_tool_use_id);
        activity.metadata.agent_name = label::sanitize(Some(row.agent_name), row.ord);
        activity.metadata.dispatch_description = label::sanitize(row.description, row.ord);
        page.relationships.push(build::relationship(
            &activity,
            FlowRelationshipKind::DispatchTool,
            "tool",
            Some(activity.id.clone()),
            Some(format!("tools.uses:{}:observed", row.tool_use_id)),
        ));
        push_correlated_worker(
            page,
            &activity,
            &actor,
            CorrelatedWorkerSignals {
                instance_id: row.instance_id,
                persona: row.persona,
                worker_producer_id: row.worker_producer_id,
            },
        );
        page.actors.push(actor);
        page.activities.push(activity);
    }
    Ok(())
}

struct CorrelatedWorkerSignals {
    instance_id: Option<i64>,
    persona: Option<String>,
    worker_producer_id: Option<String>,
}

fn push_correlated_worker(
    page: &mut FlowPage,
    dispatch_activity: &FlowActivity,
    dispatch_actor: &FlowActor,
    signals: CorrelatedWorkerSignals,
) {
    if let Some(producer) = signals.worker_producer_id.as_deref() {
        push_producer_worker(page, dispatch_activity, dispatch_actor, producer);
    }
    let Some(instance_id) = signals.instance_id else {
        return;
    };
    let persona = build::vocabulary(signals.persona);
    let mut worker = build::actor(
        page.session_id,
        FlowActorKind::Worker,
        format!("worker:{}:instance:{}", page.session_id, instance_id),
        persona.unwrap_or_else(|| format!("Worker {instance_id}")),
        Some(build::source(
            "agents.instances",
            instance_id,
            page.session_id,
            None,
        )),
    );
    worker.provenance = FlowProvenance::ExplicitCorrelation;
    worker.parent_actor_id = Some(dispatch_actor.id.clone());
    worker.producer_id = signals.worker_producer_id;
    page.relationships.push(build::relationship(
        dispatch_activity,
        FlowRelationshipKind::ExplicitParent,
        "worker",
        Some(dispatch_activity.id.clone()),
        Some(worker.id.clone()),
    ));
    page.actors.push(worker);
}

fn push_producer_worker(
    page: &mut FlowPage,
    dispatch_activity: &FlowActivity,
    dispatch_actor: &FlowActor,
    producer: &str,
) {
    let mut worker = build::actor(
        page.session_id,
        FlowActorKind::Worker,
        format!("worker:{}:{producer}", page.session_id),
        "Worker".into(),
        dispatch_actor.source.clone(),
    );
    worker.provenance = FlowProvenance::ExplicitCorrelation;
    worker.producer_id = Some(producer.to_string());
    page.relationships.push(build::relationship(
        dispatch_activity,
        FlowRelationshipKind::ExplicitParent,
        "producer",
        Some(dispatch_activity.id.clone()),
        Some(worker.id.clone()),
    ));
    page.actors.push(worker);
}
