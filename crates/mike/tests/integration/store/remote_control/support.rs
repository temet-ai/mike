use chrono::Utc;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::remote_control::{RemoteControlCall, RemoteControlSignals};
use mike::store::persistence::steering::NewIntervention;
use mike::store::test_support::{TestDb, seed_project};

pub async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(handle))
}

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn unique(label: &str) -> String {
    format!("{label}_{}", Uuid::new_v4().simple())
}

pub fn call_at<'a>(cse_id: &'a str, method: &'a str, path: &'a str) -> RemoteControlCall<'a> {
    RemoteControlCall {
        cse_id,
        method,
        path,
        http_status: Some(200),
        request_body: None,
        request_body_sha256: None,
        request_body_size: None,
        response_body: None,
        response_body_sha256: None,
        response_body_size: None,
        created_at: Utc::now(),
        completed_at: Some(Utc::now()),
        signals: RemoteControlSignals::default(),
        payloads: &[],
    }
}

pub async fn seed_session(handle: &PersistHandle, project_path: &str) -> anyhow::Result<Uuid> {
    let project_id = seed_project(handle.pool(), project_path).await?;
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions.sessions(session_uuid, project_id) VALUES ($1, $2)")
        .bind(session)
        .bind(project_id)
        .execute(handle.pool())
        .await?;
    Ok(session)
}

pub async fn seed_channel(handle: &PersistHandle, session: Uuid) -> anyhow::Result<String> {
    let cse_id = unique("cse_injection");
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1,$2)")
        .bind(&cse_id)
        .bind(session)
        .execute(handle.pool())
        .await?;
    Ok(cse_id)
}

pub async fn seed_message_with_channel(
    handle: &PersistHandle,
    project_path: &str,
) -> anyhow::Result<(Uuid, Uuid, String)> {
    let session = seed_session(handle, project_path).await?;
    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message, session, None, "test envelope")
        .await?;
    let cse_id = seed_channel(handle, session).await?;
    Ok((session, message, cse_id))
}

pub async fn seed_steerer_intervention(
    handle: &PersistHandle,
) -> anyhow::Result<(Uuid, i64, Uuid, String)> {
    let session = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar(
        "WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ($1, $1) RETURNING id) \
         INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $2, id FROM p RETURNING id",
    )
    .bind(format!("/tmp/rc-steerer-{session}"))
    .bind(session)
    .fetch_one(handle.pool())
    .await?;
    let cse_id = unique("cse_injection");
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1,$2)")
        .bind(&cse_id)
        .bind(session)
        .execute(handle.pool())
        .await?;
    let steerer_id: i64 =
        sqlx::query_scalar("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(handle.pool())
            .await?;
    let intervention_uuid = Uuid::new_v4();
    handle
        .steerer()
        .insert_intervention(NewIntervention {
            steerer_id,
            enrolment_id: None,
            session_id,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "redirect",
            trigger_request_id: None,
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("nudge text"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid,
            expires_at: Utc::now() + chrono::Duration::minutes(15),
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    Ok((session, session_id, intervention_uuid, cse_id))
}
