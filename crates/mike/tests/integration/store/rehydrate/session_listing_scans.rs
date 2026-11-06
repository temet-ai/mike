use chrono::Utc;

use mike::store::persistence::rehydrate::{fetch_session_summary, list_sessions_paginated};
use mike::store::test_support::TestDb;
use mike::types::SortOrder;

use crate::store::rehydrate::support::{
    SeedRequest, seed_project, seed_request, seed_request_with_status, seed_session,
};

async fn isolated_pool()
-> anyhow::Result<Option<(sqlx::PgPool, mike::store::test_support::TestDbHandle)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    Ok(Some((pool, db)))
}

#[tokio::test]
async fn session_lists_derive_last_status_live_not_from_stale_cache() -> anyhow::Result<()> {
    let Some((pool, db)) = isolated_pool().await? else {
        return Ok(());
    };
    let project_id = seed_project(&pool, "session-lists-last-status-proj").await?;

    let stale_pending_uuid = uuid::Uuid::new_v4();
    let stale_pending_session = seed_session(&pool, stale_pending_uuid, project_id).await?;
    seed_request_with_status(
        &pool,
        stale_pending_session,
        project_id,
        "complete",
        Some(Utc::now()),
        None,
    )
    .await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_status = 'Pending' WHERE id = $1",
        stale_pending_session,
    )
    .execute(&pool)
    .await?;

    let stale_terminal_uuid = uuid::Uuid::new_v4();
    let stale_terminal_session = seed_session(&pool, stale_terminal_uuid, project_id).await?;
    seed_request_with_status(
        &pool,
        stale_terminal_session,
        project_id,
        "complete",
        Some(Utc::now()),
        None,
    )
    .await?;
    seed_request_with_status(
        &pool,
        stale_terminal_session,
        project_id,
        "error",
        Some(Utc::now()),
        Some("proxy_crashed_during_stream"),
    )
    .await?;
    sqlx::query!(
        "UPDATE sessions.sessions SET last_status = 'Complete' WHERE id = $1",
        stale_terminal_session,
    )
    .execute(&pool)
    .await?;

    for (uuid, db_id, expected, label) in [
        (
            stale_pending_uuid,
            stale_pending_session,
            "Complete",
            "stale_pending_session (cache says Pending, newest request is Complete)",
        ),
        (
            stale_terminal_uuid,
            stale_terminal_session,
            "Error: proxy_crashed_during_stream",
            "stale_terminal_session (cache says Complete, newest request errored)",
        ),
    ] {
        let summary = fetch_session_summary(&pool, db_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("{label}: fetch_session_summary must find a row"))?;
        assert_eq!(
            summary.last_status.as_deref(),
            Some(expected),
            "{label}: fetch_session_summary must report the live status"
        );

        let paginated = list_sessions_paginated(&pool, 0, 50, SortOrder::Desc, None, None).await?;
        let row = paginated
            .iter()
            .find(|r| r.session_id == uuid.to_string())
            .ok_or_else(|| anyhow::anyhow!("{label}: list_sessions_paginated must list it"))?;
        assert_eq!(
            row.last_status.as_deref(),
            Some(expected),
            "{label}: list_sessions_paginated must report the live status"
        );
    }

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_paginated_projects_touched_reflects_request_level_divergence()
-> anyhow::Result<()> {
    let Some((pool, db)) = isolated_pool().await? else {
        return Ok(());
    };

    let session_project = seed_project(&pool, "issue32-session-home").await?;
    let nested_a = seed_project(&pool, "issue32-nested-a").await?;
    let nested_b = seed_project(&pool, "issue32-nested-b").await?;
    let session_uuid = uuid::Uuid::new_v4();
    let session_fk = seed_session(&pool, session_uuid, session_project).await?;

    let base = Utc::now();
    for (idx, project_id) in [nested_a, nested_a, nested_b].into_iter().enumerate() {
        seed_request(
            &pool,
            SeedRequest {
                session_id_fk: session_fk,
                project_id,
                model: None,
                turn_origin: "primary",
                created_at: base + chrono::Duration::seconds(idx as i64),
                agent_instance_id: None,
                human_prompt: "",
            },
        )
        .await?;
    }

    let rows = list_sessions_paginated(&pool, 0, 50, SortOrder::Desc, None, None).await?;
    let row = rows
        .iter()
        .find(|r| r.session_id == session_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("seeded session must appear in the page"))?;

    assert_eq!(
        row.project_path, "issue32-session-home",
        "session-level project_path must stay the session's own resolved project"
    );
    let mut touched = row.projects_touched.clone();
    touched.sort();
    assert_eq!(
        touched,
        vec![
            "issue32-nested-a".to_string(),
            "issue32-nested-b".to_string()
        ],
        "projects_touched must be the distinct set from the session's own requests, \
         not the session's own project_id"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_sessions_paginated_names_a_session_from_its_own_first_prompt() -> anyhow::Result<()> {
    let Some((pool, db)) = isolated_pool().await? else {
        return Ok(());
    };

    let project = seed_project(&pool, "issue31-naming").await?;
    let base = Utc::now();

    let subagent_uuid = uuid::Uuid::new_v4();
    let subagent_fk = seed_session(&pool, subagent_uuid, project).await?;
    for (idx, (origin, prompt)) in [
        ("subagent", "Audit the retry path and report what you find"),
        ("subagent", "second turn, later, must not win"),
    ]
    .into_iter()
    .enumerate()
    {
        seed_request(
            &pool,
            SeedRequest {
                session_id_fk: subagent_fk,
                project_id: project,
                model: None,
                turn_origin: origin,
                created_at: base + chrono::Duration::seconds(idx as i64),
                agent_instance_id: None,
                human_prompt: prompt,
            },
        )
        .await?;
    }

    let probe_uuid = uuid::Uuid::new_v4();
    let probe_fk = seed_session(&pool, probe_uuid, project).await?;
    seed_request(
        &pool,
        SeedRequest {
            session_id_fk: probe_fk,
            project_id: project,
            model: None,
            turn_origin: "quota_probe",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "quota",
        },
    )
    .await?;

    let primary_uuid = uuid::Uuid::new_v4();
    let primary_fk = seed_session(&pool, primary_uuid, project).await?;
    seed_request(
        &pool,
        SeedRequest {
            session_id_fk: primary_fk,
            project_id: project,
            model: None,
            turn_origin: "primary_first",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "fix the retry backoff",
        },
    )
    .await?;

    let rows = list_sessions_paginated(&pool, 0, 200, SortOrder::Desc, None, None).await?;
    let prompt_for = |uuid: uuid::Uuid| -> Option<String> {
        rows.iter()
            .find(|r| r.session_id == uuid.to_string())
            .and_then(|r| r.originating_operator_prompt.clone())
    };

    assert_eq!(
        prompt_for(subagent_uuid).as_deref(),
        Some("Audit the retry path and report what you find"),
        "a dispatched subagent must be named from its own first captured prompt, \
         not left to render as a hex id"
    );
    assert_eq!(
        prompt_for(primary_uuid).as_deref(),
        Some("fix the retry backoff"),
        "a primary-lane session with no classification yet must still be named"
    );
    assert_eq!(
        prompt_for(probe_uuid),
        None,
        "a session whose only text is a quota probe stays unnamed — probe text is \
         harness machinery, not the session's subject"
    );

    let summary = fetch_session_summary(&pool, subagent_fk).await?;
    assert_eq!(
        summary
            .as_ref()
            .and_then(|s| s.originating_operator_prompt.as_deref()),
        Some("Audit the retry path and report what you find"),
        "fetch_session_summary must resolve the same name as the list query"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
