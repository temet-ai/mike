use chrono::Utc;

use mike::store::persistence::rehydrate::{
    fetch_predecessor_request_id, fetch_session_request_summaries_paginated,
};
use mike::types::SortOrder;

use crate::store::rehydrate::support::{
    seed_agent, seed_agent_instance, seed_project, seed_scoped_request, seed_session, shared_pool,
    unique,
};

#[tokio::test]
async fn predecessor_request_id_real_fn_both_branches() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("pred-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let p1 = seed_scoped_request(&pool, session_fk, project_id, "primary", base, None).await?;
    let p2 = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "primary_first",
        base + chrono::Duration::seconds(1),
        None,
    )
    .await?;

    let pred =
        fetch_predecessor_request_id(&pool, session_fk, base + chrono::Duration::seconds(2), None)
            .await?;
    assert_eq!(
        pred,
        Some(p2),
        "primary branch resolves the chrono-prior primary-lineage request"
    );

    let pred_early = fetch_predecessor_request_id(
        &pool,
        session_fk,
        base + chrono::Duration::milliseconds(500),
        None,
    )
    .await?;
    assert_eq!(
        pred_early,
        Some(p1),
        "earlier cutoff resolves the first primary"
    );

    let agent_id = seed_agent(&pool).await?;
    let instance_id = seed_agent_instance(&pool, session_fk, p1, agent_id, 0).await?;

    let sa1 = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "subagent",
        base + chrono::Duration::seconds(3),
        Some(instance_id),
    )
    .await?;
    let other_instance_id = seed_agent_instance(&pool, session_fk, p1, agent_id, 1).await?;
    let _other_sa = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "subagent",
        base + chrono::Duration::seconds(4),
        Some(other_instance_id),
    )
    .await?;

    let pred_sa = fetch_predecessor_request_id(
        &pool,
        session_fk,
        base + chrono::Duration::seconds(5),
        Some(instance_id),
    )
    .await?;
    assert_eq!(
        pred_sa,
        Some(sa1),
        "sub-agent branch scopes to the matching agent_instance_id only"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn dispatch_request_id_decoupled_from_predecessor_for_subagent_row() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("dispatch-anchor-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let dispatcher_id =
        seed_scoped_request(&pool, session_fk, project_id, "primary", base, None).await?;

    let agent_id = seed_agent(&pool).await?;
    let instance_id = seed_agent_instance(&pool, session_fk, dispatcher_id, agent_id, 0).await?;

    let subagent_id = seed_scoped_request(
        &pool,
        session_fk,
        project_id,
        "subagent",
        base + chrono::Duration::seconds(1),
        Some(instance_id),
    )
    .await?;

    let (rows, _total) =
        fetch_session_request_summaries_paginated(&pool, session_fk, 0, 50, SortOrder::Asc).await?;
    let subagent_row = rows
        .iter()
        .find(|r| r.id == subagent_id as u64)
        .ok_or_else(|| anyhow::anyhow!("subagent row must be present in the session listing"))?;

    assert_eq!(
        subagent_row.dispatch_request_id,
        Some(dispatcher_id),
        "dispatch_request_id resolves to the dispatching primary request"
    );
    assert!(
        subagent_row.predecessor_request_id.is_none(),
        "predecessor_request_id stays None for a subagent row — decoupled from dispatch_request_id"
    );

    pool.close().await;
    Ok(())
}
