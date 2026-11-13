use crate::store::persistence::launch_repo::LaunchPersist as _;
use bytes::Bytes;
use chrono::Utc;
use serde_json::json;

use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support as test_util;
use crate::store::persistence::work_items::{LaunchRecord, insert_launch};
use crate::work_items::output_end::Ended;
use crate::work_items::parse::{Launch, WorkKind};

const REQUEST: u64 = 98_001;
const KILLED: Ended = Ended {
    state: "killed",
    exit_code: None,
};

async fn session_with_a_request(handle: &PersistHandle) -> anyhow::Result<i64> {
    let session = uuid::Uuid::new_v4().to_string();
    let mut req =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(REQUEST)), &session);
    req.request_body = Some(Bytes::from(serde_json::to_vec(&json!({"messages": []}))?));
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session)
            .fetch_one(&handle.pool)
            .await?,
    )
}

async fn launch(
    handle: &PersistHandle,
    session_db_id: i64,
    (kind, task, wire): (WorkKind, &str, &str),
    output_path: Option<&str>,
) -> anyhow::Result<()> {
    let mut conn = handle.pool.acquire().await?;
    let record = LaunchRecord {
        session_db_id,
        wire_tool_use_id: wire,
        launch: Launch {
            kind,
            task_id: task.to_string(),
            output_path: output_path.map(str::to_string),
        },
        description: None,
        owner_agent_id: None,
        request_db_id: i64::try_from(REQUEST)?,
        at: Utc::now() - chrono::Duration::minutes(10),
    };
    insert_launch(&mut conn, &record).await?;
    Ok(())
}

#[tokio::test]
async fn a_running_job_with_an_output_file_is_listed_and_closed_once() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = session_with_a_request(&handle).await?;
    launch(
        &handle,
        session,
        (WorkKind::Bash, "job1", "toolu_job1"),
        Some("/tmp/x/job1.output"),
    )
    .await?;
    launch(
        &handle,
        session,
        (WorkKind::Bash, "job2", "toolu_job2"),
        None,
    )
    .await?;
    launch(
        &handle,
        session,
        (WorkKind::Agent, "agent1", "toolu_agent1"),
        Some("/tmp/x/agent1.output"),
    )
    .await?;

    let open = handle.launch().open_background_jobs(session).await?;
    assert_eq!(open.len(), 1, "only a bash job with an output file is read");
    assert_eq!(open[0].output_path, "/tmp/x/job1.output");

    let at = Utc::now() - chrono::Duration::minutes(5);
    assert_eq!(
        handle
            .launch()
            .close_job_from_its_output(open[0].id, KILLED, at)
            .await?,
        1
    );
    assert_eq!(
        handle
            .launch()
            .close_job_from_its_output(open[0].id, KILLED, at)
            .await?,
        0,
        "a job that is no longer running is left as it is"
    );
    assert!(
        handle
            .launch()
            .open_background_jobs(session)
            .await?
            .is_empty()
    );
    let (state, summary): (String, Option<String>) =
        sqlx::query_as("SELECT state, summary FROM agents.work_items WHERE id = $1")
            .bind(open[0].id)
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(state, "killed");
    assert!(summary.is_some_and(|s| s.contains("output file")));

    assert!(handle.launch().open_background_jobs(-1).await?.is_empty());
    db.cleanup().await?;
    Ok(())
}
