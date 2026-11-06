use mike::store::persistence::session_flow::{
    FlowActorKind, FlowComponent, FlowProvenance, FlowRelationshipKind,
};

use super::support::Fixture;

#[tokio::test]
async fn dispatch_with_worker_producer_id_links_to_producer_actor_and_tags_instance_worker()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let emitter = f.request(None, "primary").await?;
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (request_id, session_id, ord, wire_tool_use_id, tool_name, input, created_at)
         VALUES ($1, $2, 0, 'producer-probe', 'Agent', '{}', '2026-09-10T12:00:02Z') RETURNING id",
    )
    .bind(emitter)
    .bind(f.id)
    .fetch_one(f.handle.pool())
    .await?;
    let agent_id: i64 = f.agent("reviewer").await?;
    let producer = "a53fd635aaceeed5c";
    let dispatch_id: i64 = sqlx::query_scalar(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id, worker_producer_id)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tool_use_id)
    .bind(agent_id)
    .bind(producer)
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
    let dispatch_actor_id = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::UnresolvedDispatch)
        .ok_or_else(|| anyhow::anyhow!("dispatch actor missing"))?
        .id
        .clone();

    let producer_actor = page
        .actors
        .iter()
        .find(|a| a.id == format!("worker:{}:{producer}", f.session))
        .ok_or_else(|| anyhow::anyhow!("producer-keyed worker actor missing"))?;
    assert_eq!(producer_actor.producer_id.as_deref(), Some(producer));
    assert_eq!(
        producer_actor.provenance,
        FlowProvenance::ExplicitCorrelation
    );

    let instance_worker = page
        .actors
        .iter()
        .find(|a| a.kind == FlowActorKind::Worker && a.id != producer_actor.id)
        .ok_or_else(|| anyhow::anyhow!("instance-correlated worker actor missing"))?;
    assert_eq!(instance_worker.producer_id.as_deref(), Some(producer));
    assert_eq!(instance_worker.parent_actor_id, Some(dispatch_actor_id));

    let explicit_parent_targets: Vec<&str> = page
        .relationships
        .iter()
        .filter(|r| r.kind == FlowRelationshipKind::ExplicitParent)
        .filter_map(|r| r.to_id.as_deref())
        .collect();
    assert!(explicit_parent_targets.contains(&producer_actor.id.as_str()));
    assert!(explicit_parent_targets.contains(&instance_worker.id.as_str()));

    let page_again = f.page(FlowComponent::Dispatches, 10, None).await?;
    assert_eq!(page_again.actors, page.actors);
    assert_eq!(page_again.relationships, page.relationships);

    f.cleanup().await
}
