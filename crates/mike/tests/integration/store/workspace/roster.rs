use chrono::{DateTime, SubsecRound, Utc};

use mike::workspace::{Liveness, RosterScope};

use crate::store::workspace::support::{
    register_input, shared_handle, shared_pool_and_url, unique,
};

async fn seed_activity_request(
    pool: &sqlx::PgPool,
    session_uuid: uuid::Uuid,
    project_id: i64,
    created_at: DateTime<Utc>,
    request_number: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id,
                params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
                created_at, completed_at, request_number
             )
             SELECT ss.id, $2, rm.id, '/v1/messages', false,
                    0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                    $3, $3, $4
               FROM sessions.sessions ss,
                    requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE ss.session_uuid = $1
                AND rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
        session_uuid,
        project_id,
        created_at,
        request_number,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn fetch_session_liveness_reflects_registration_state() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let project_path = unique("liveness-test");

    let never_seen = uuid::Uuid::new_v4();
    assert_eq!(handle.fetch_session_liveness(never_seen).await?, None);

    let session = uuid::Uuid::new_v4();
    handle
        .register_workspace_session(register_input(session, &project_path))
        .await?;
    assert_eq!(
        handle.fetch_session_liveness(session).await?,
        Some(Liveness::Live)
    );

    sqlx::query(
        "UPDATE workspace.session_registrations SET lease_expires_at = now() - INTERVAL '20 minutes'
             WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session)
    .execute(&pool)
    .await?;
    assert_eq!(
        handle.fetch_session_liveness(session).await?,
        Some(Liveness::Stale)
    );

    handle.end_workspace_registration(session, None).await?;
    assert_eq!(
        handle.fetch_session_liveness(session).await?,
        Some(Liveness::Ended)
    );

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_workspace_roster_filters_by_scope() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let project_path = unique("roster-scope-test");
    let live_session = uuid::Uuid::new_v4();
    let stale_session = uuid::Uuid::new_v4();
    let ended_session = uuid::Uuid::new_v4();

    for session in [live_session, stale_session, ended_session] {
        handle
            .register_workspace_session(register_input(session, &project_path))
            .await?;
    }
    let project_id = handle
        .find_project_id_by_path(&project_path)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project not found after registration"))?;

    sqlx::query!(
        "UPDATE workspace.session_registrations SET lease_expires_at = now() - INTERVAL '20 minutes'
             WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
        stale_session,
    )
    .execute(&pool)
    .await?;

    handle
        .end_workspace_registration(ended_session, None)
        .await?;

    let live_roster = handle
        .fetch_workspace_roster(project_id, RosterScope::Live)
        .await?;
    assert_eq!(
        live_roster
            .iter()
            .map(|r| r.session_uuid)
            .collect::<Vec<_>>(),
        vec![live_session],
        "Live scope returns only the strictly-live row"
    );

    let active_roster = handle
        .fetch_workspace_roster(project_id, RosterScope::Active)
        .await?;
    let mut active_uuids: Vec<_> = active_roster.iter().map(|r| r.session_uuid).collect();
    active_uuids.sort();
    let mut expected_active = vec![live_session, stale_session];
    expected_active.sort();
    assert_eq!(
        active_uuids, expected_active,
        "Active scope returns live+stale, never ended"
    );

    let all_roster = handle
        .fetch_workspace_roster(project_id, RosterScope::All)
        .await?;
    let mut all_uuids: Vec<_> = all_roster.iter().map(|r| r.session_uuid).collect();
    all_uuids.sort();
    let mut expected_all = vec![live_session, stale_session, ended_session];
    expected_all.sort();
    assert_eq!(all_uuids, expected_all, "All scope returns every row");

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_workspace_roster_reports_activity_for_idle_and_busy_sessions() -> anyhow::Result<()>
{
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let idle_path = unique("roster-activity-idle");
    let idle_session = uuid::Uuid::new_v4();
    let idle_registered = handle
        .register_workspace_session(register_input(idle_session, &idle_path))
        .await?;

    let idle_roster = handle
        .fetch_workspace_roster(idle_registered.project_id, RosterScope::Live)
        .await?;
    let idle_row = idle_roster
        .iter()
        .find(|r| r.session_uuid == idle_session)
        .ok_or_else(|| anyhow::anyhow!("idle session missing from roster"))?;
    assert!(
        idle_row.last_request_at.is_none(),
        "a session with zero captured requests must report no last_request_at: {idle_row:?}"
    );
    assert!(
        idle_row.newest_request_number.is_none(),
        "a session with zero captured requests must report no newest_request_number: {idle_row:?}"
    );

    let busy_path = unique("roster-activity-busy");
    let busy_session = uuid::Uuid::new_v4();
    let busy_registered = handle
        .register_workspace_session(register_input(busy_session, &busy_path))
        .await?;

    let now = Utc::now().trunc_subsecs(6);
    seed_activity_request(
        &pool,
        busy_session,
        busy_registered.project_id,
        now - chrono::Duration::minutes(5),
        1,
    )
    .await?;
    let newest_at = now - chrono::Duration::seconds(30);
    seed_activity_request(
        &pool,
        busy_session,
        busy_registered.project_id,
        newest_at,
        2,
    )
    .await?;

    let busy_roster = handle
        .fetch_workspace_roster(busy_registered.project_id, RosterScope::Live)
        .await?;
    let busy_row = busy_roster
        .iter()
        .find(|r| r.session_uuid == busy_session)
        .ok_or_else(|| anyhow::anyhow!("busy session missing from roster"))?;
    assert_eq!(
        busy_row.last_request_at,
        Some(newest_at),
        "must report the chronologically newest request, not the first: {busy_row:?}"
    );
    assert_eq!(
        busy_row.newest_request_number,
        Some(2),
        "must report the newest request's ordinal: {busy_row:?}"
    );

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_workspace_roster_live_scope_keeps_lapsed_lease_row_with_recent_activity()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let project_path = unique("roster-activity-membership");
    let busy_but_stale = uuid::Uuid::new_v4();
    let quiet_no_requests = uuid::Uuid::new_v4();
    let quiet_old_activity = uuid::Uuid::new_v4();

    let mut project_id = None;
    for session in [busy_but_stale, quiet_no_requests, quiet_old_activity] {
        let registered = handle
            .register_workspace_session(register_input(session, &project_path))
            .await?;
        project_id = Some(registered.project_id);
    }
    let project_id = project_id.ok_or_else(|| anyhow::anyhow!("no session registered"))?;

    sqlx::query!(
        "UPDATE workspace.session_registrations
                SET lease_expires_at = now() - INTERVAL '20 minutes'
              WHERE session_id IN (
                  SELECT id FROM sessions.sessions WHERE session_uuid = ANY($1)
              )",
        &[busy_but_stale, quiet_no_requests, quiet_old_activity][..],
    )
    .execute(&pool)
    .await?;

    let now = Utc::now();
    seed_activity_request(
        &pool,
        busy_but_stale,
        project_id,
        now - chrono::Duration::minutes(2),
        1,
    )
    .await?;
    seed_activity_request(
        &pool,
        quiet_old_activity,
        project_id,
        now - chrono::Duration::minutes(20),
        1,
    )
    .await?;

    let live_roster = handle
        .fetch_workspace_roster(project_id, RosterScope::Live)
        .await?;
    let live_uuids: std::collections::HashSet<_> =
        live_roster.iter().map(|r| r.session_uuid).collect();
    assert!(
        live_uuids.contains(&busy_but_stale),
        "a lapsed-lease session with a request inside the activity window must stay on the default roster: {live_uuids:?}"
    );
    assert!(
        !live_uuids.contains(&quiet_no_requests),
        "a lapsed-lease session with zero requests must not appear on the default roster: {live_uuids:?}"
    );
    assert!(
        !live_uuids.contains(&quiet_old_activity),
        "a lapsed-lease session whose last request predates the activity window must not appear on the default roster: {live_uuids:?}"
    );
    let busy_row = live_roster
        .iter()
        .find(|r| r.session_uuid == busy_but_stale)
        .ok_or_else(|| anyhow::anyhow!("busy_but_stale missing from Live roster"))?;
    assert!(
        matches!(busy_row.liveness, Liveness::Live),
        "a channel-less session issuing requests inside the activity window is live even with a lapsed lease: {busy_row:?}"
    );
    assert_eq!(busy_row.liveness_source, "activity", "{busy_row:?}");
    assert_eq!(
        handle.fetch_session_liveness(busy_but_stale).await?,
        Some(Liveness::Live),
        "the mailbox and fleet graph liveness lookup must agree with the roster"
    );
    assert_eq!(
        handle.fetch_session_liveness(quiet_old_activity).await?,
        Some(Liveness::Stale)
    );

    let active_roster = handle
        .fetch_workspace_roster(project_id, RosterScope::Active)
        .await?;
    let active_uuids: std::collections::HashSet<_> =
        active_roster.iter().map(|r| r.session_uuid).collect();
    assert!(active_uuids.contains(&busy_but_stale));
    assert!(active_uuids.contains(&quiet_no_requests));
    assert!(active_uuids.contains(&quiet_old_activity));

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn registered_session_without_captured_traffic_is_uncaptured_not_live() -> anyhow::Result<()>
{
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let Some((pool, _)) = shared_pool_and_url().await? else {
        return Ok(());
    };

    let project_path = unique("roster-uncaptured");
    let fresh = uuid::Uuid::new_v4();
    let uncaptured = uuid::Uuid::new_v4();
    let captured = uuid::Uuid::new_v4();
    let mut project_id = None;
    for session in [fresh, uncaptured, captured] {
        let registered = handle
            .register_workspace_session(register_input(session, &project_path))
            .await?;
        project_id = Some(registered.project_id);
    }
    let project_id = project_id.ok_or_else(|| anyhow::anyhow!("no session registered"))?;

    sqlx::query!(
        "UPDATE workspace.session_registrations
                SET registered_at = now() - INTERVAL '30 minutes'
              WHERE session_id IN (
                  SELECT id FROM sessions.sessions WHERE session_uuid = ANY($1)
              )",
        &[uncaptured, captured][..],
    )
    .execute(&pool)
    .await?;
    seed_activity_request(
        &pool,
        captured,
        project_id,
        Utc::now() - chrono::Duration::minutes(1),
        1,
    )
    .await?;

    assert_eq!(
        handle.fetch_session_liveness(uncaptured).await?,
        Some(Liveness::Uncaptured)
    );
    assert_eq!(
        handle.fetch_session_liveness(fresh).await?,
        Some(Liveness::Live),
        "a session registered moments ago has not had time to send a request"
    );
    assert_eq!(
        handle.fetch_session_liveness(captured).await?,
        Some(Liveness::Live)
    );

    let live = handle
        .fetch_workspace_roster(project_id, RosterScope::Live)
        .await?;
    let live_uuids: std::collections::HashSet<_> = live.iter().map(|r| r.session_uuid).collect();
    assert!(!live_uuids.contains(&uncaptured), "{live_uuids:?}");
    assert!(live_uuids.contains(&fresh) && live_uuids.contains(&captured));

    let active = handle
        .fetch_workspace_roster(project_id, RosterScope::Active)
        .await?;
    let row = active
        .iter()
        .find(|r| r.session_uuid == uncaptured)
        .ok_or_else(|| anyhow::anyhow!("uncaptured session missing from Active roster"))?;
    assert_eq!(row.liveness, Liveness::Uncaptured, "{row:?}");

    let nodes = handle.fetch_fleet_nodes(project_id, 24).await?;
    let node = nodes
        .iter()
        .find(|n| n.session_uuid == uncaptured)
        .ok_or_else(|| anyhow::anyhow!("uncaptured session missing from fleet nodes"))?;
    assert_eq!(node.registration_liveness, Some(Liveness::Uncaptured));

    pool.close().await;
    handle.pool().close().await;
    Ok(())
}
