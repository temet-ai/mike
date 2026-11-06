use crate::store::persistence::steering::NewIntervention;
use crate::store::persistence::{PersistHandle, test_support};

async fn seed_project_session(pool: &sqlx::PgPool) -> anyhow::Result<(i64, i64)> {
    let project_id =
        test_support::seed_project(pool, &format!("proj-machine-text-{}", uuid::Uuid::new_v4()))
            .await?;
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        uuid::Uuid::new_v4(),
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok((project_id, session_id))
}

async fn seed_request(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_id: i64,
    human_prompt: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO requests.requests
                   (session_id, project_id, method_id, path, is_streaming,
                    message_count, is_tool_loop, status_id, params,
                    last_user_message, human_prompt, turn_type_id, turn_origin_id)
               SELECT $1, $2, rm.id, '/v1/messages', false,
                      0, false, rs.id, '{}'::jsonb,
                      '', $3, rtt.id, rto.id
                 FROM requests.methods rm, requests.statuses rs,
                      requests.turn_types rtt, requests.turn_origins rto
                WHERE rm.code = 'POST' AND rs.code = 'complete'
                  AND rtt.code = 'user_prompt' AND rto.code = 'primary'
               RETURNING id"#,
        session_id,
        project_id,
        human_prompt,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn seed_intervention(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    session_id: i64,
    trigger_request_id: i64,
) -> anyhow::Result<uuid::Uuid> {
    let steerer_id: i64 =
        sqlx::query_scalar!("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(pool)
            .await?;
    let intervention_uuid = uuid::Uuid::new_v4();
    handle
        .insert_intervention(NewIntervention {
            steerer_id,
            enrolment_id: None,
            session_id,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "redirect",
            trigger_request_id: Some(trigger_request_id),
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("nudge text"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    Ok(intervention_uuid)
}

async fn assert_prompt_does_not_supersede(human_prompt: &str, label: &str) -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping {label}");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (project_id, session_id) = seed_project_session(&pool).await?;
    let trigger_id = seed_request(&pool, project_id, session_id, "").await?;
    let intervention_uuid = seed_intervention(&handle, &pool, session_id, trigger_id).await?;

    seed_request(&pool, project_id, session_id, human_prompt).await?;

    let staleness = handle
        .intervention_staleness(intervention_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded intervention must be found"))?;

    assert!(
        !staleness.superseded,
        "{label}: machine-authored prompt must not count as an operator supersession"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn channel_wake_does_not_supersede() -> anyhow::Result<()> {
    assert_prompt_does_not_supersede(
        "<channel source=\"wake\" reason=\"steering\" pending=\"1\">check in</channel>",
        "channel wake",
    )
    .await
}

#[tokio::test]
async fn task_notification_does_not_supersede() -> anyhow::Result<()> {
    assert_prompt_does_not_supersede(
        "<task-notification>\n<task-id>abc</task-id>\n<status>completed</status>\n\
         </task-notification>",
        "task notification",
    )
    .await
}

#[tokio::test]
async fn mailbox_delivery_does_not_supersede() -> anyhow::Result<()> {
    assert_prompt_does_not_supersede(
        "<system-reminder>\nCross-session message from session abc123, queued \
         2026-08-03T00:00:00Z. To reply, use the `send_session_message` tool with \
         to=\"abc123\".\n\nbody\n</system-reminder>",
        "mailbox delivery",
    )
    .await
}

#[tokio::test]
async fn stepped_away_recap_does_not_supersede() -> anyhow::Result<()> {
    assert_prompt_does_not_supersede(
        "The user stepped away and is coming back. Recap in under 40 words what you \
         were doing.",
        "stepped-away recap",
    )
    .await
}

#[tokio::test]
async fn steering_payload_echo_does_not_supersede() -> anyhow::Result<()> {
    assert_prompt_does_not_supersede(
        "<system-reminder>Steering note from the operator's proxy:\n\nfocus on X\n\n\
         Act on it, or say briefly why it doesn't apply.</system-reminder>",
        "steering payload echo",
    )
    .await
}

#[tokio::test]
async fn genuine_operator_prompt_still_supersedes() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping genuine_operator_prompt_still_supersedes");
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (project_id, session_id) = seed_project_session(&pool).await?;
    let trigger_id = seed_request(&pool, project_id, session_id, "").await?;
    let intervention_uuid = seed_intervention(&handle, &pool, session_id, trigger_id).await?;

    seed_request(
        &pool,
        project_id,
        session_id,
        "please add a retry to the http client",
    )
    .await?;

    let staleness = handle
        .intervention_staleness(intervention_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded intervention must be found"))?;

    assert!(
        staleness.superseded,
        "a genuine operator prompt must still supersede a pending payload"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
