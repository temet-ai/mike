pub(super) async fn seed_session(pool: &sqlx::PgPool, label: &str) -> anyhow::Result<uuid::Uuid> {
    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        label,
    )
    .fetch_one(pool)
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW())",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

pub(super) async fn drain_primary_turn(
    state: &crate::config::AppState,
    session: &str,
) -> Vec<crate::store::models::SystemReminder> {
    let mut body = bytes::Bytes::from_static(
        br#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":[{"type":"text","text":"q"}]}]}"#,
    );
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = crate::compress::manifest::ManifestBuilder::new("test".to_string());
    let mut bk = crate::reminders::inject::InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: crate::store::models::RequestId::next(),
    };
    crate::steerer::stage::drain(
        state,
        session,
        &crate::extract::request::TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await
}
