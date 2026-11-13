use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch::{
    LaunchListFilter, LaunchOutcome, LaunchRow, LaunchSurfaceRefs, NewLaunch, StderrCapture,
};

use crate::test_db::TestDb;

fn full_launch(session: Uuid, brief: &str) -> NewLaunch {
    let mut new = NewLaunch::new("herdr", "/tmp/launch-roundtrip");
    new.session_uuid = Some(session);
    new.repo_root = Some("/tmp/launch-roundtrip".to_owned());
    new.seat = Some("db-dev".to_owned());
    new.model = Some("opus".to_owned());
    new.claude_args = vec![
        "--session-id".to_owned(),
        session.to_string(),
        "--remote-control".to_owned(),
        "--model".to_owned(),
        "opus".to_owned(),
    ];
    new.prompt_present = true;
    new.brief_mode = Some("file".to_owned());
    new.brief_path = Some("/tmp/launch-roundtrip/brief.md".to_owned());
    new.brief_text = Some(brief.to_owned());
    new.requested_placement = Some("panel".to_owned());
    new.split_direction = Some("right".to_owned());
    new
}

fn assert_pending_row(row: &LaunchRow, id: i64, session: Uuid, brief: &str) {
    assert_eq!(row.id, id);
    assert_eq!(row.result, "pending");
    assert!(!row.opened);
    assert!(row.opened_at.is_none());
    assert_eq!(row.brief_text.as_deref(), Some(brief));
    assert_eq!(
        row.claude_args,
        serde_json::json!([
            "--session-id",
            session.to_string(),
            "--remote-control",
            "--model",
            "opus"
        ])
    );
    assert!(row.remote_control && row.proxy_env);
}

fn assert_succeeded_row(row: &LaunchRow, stderr: &StderrCapture) {
    assert!(row.opened);
    assert!(row.opened_at.is_some());
    assert_eq!(row.surface_ref.as_deref(), Some("pane-7"));
    assert_eq!(row.pid, Some(4242));
    assert_eq!(row.actual_placement.as_deref(), Some("panel"));
    assert_eq!(row.result, "success");
    assert_eq!(row.delivered, Some(true));
    assert!(row.confirmed);
    assert_eq!(row.channel_check.as_deref(), Some("found"));
    assert!(row.channel_checked_at.is_some());
    assert_eq!(row.stderr_text.as_deref(), Some(stderr.text.as_str()));
    assert!(!row.stderr_truncated);
    assert!(row.closed_at.is_none());
}

fn opened_surface() -> LaunchSurfaceRefs {
    LaunchSurfaceRefs {
        workspace_ref: Some("ws-1".to_owned()),
        tab_ref: Some("tab-1".to_owned()),
        surface_ref: Some("pane-7".to_owned()),
        pid: Some(4242),
        actual_placement: Some("panel".to_owned()),
    }
}

fn confirmed_success(stderr: &StderrCapture) -> LaunchOutcome {
    LaunchOutcome {
        result: "success".to_owned(),
        reason: None,
        opened: true,
        delivered: Some(true),
        confirmed: true,
        channel_check: Some("found".to_owned()),
        stderr: Some(stderr.clone()),
    }
}

#[tokio::test]
async fn launch_lifecycle_round_trips_every_column_and_lists_newest_first() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let launch_db = handle.launch();
    let session = Uuid::new_v4();
    let brief = format!("# Brief for {session}\n{}", "detail line\n".repeat(400));

    let id = launch_db
        .insert_launch(&full_launch(session, &brief))
        .await?;

    let pending = launch_db
        .find_launch_by_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("inserted launch not found by session"))?;
    assert_pending_row(&pending, id, session, &brief);

    launch_db
        .record_launch_opened(id, &opened_surface())
        .await?;
    assert_eq!(launch_db.count_open_launches("herdr").await?, 1);
    assert_eq!(launch_db.count_open_launches("tmux").await?, 0);

    let stderr = StderrCapture::capture("warning: something\n");
    launch_db
        .record_launch_outcome(id, &confirmed_success(&stderr))
        .await?;

    let done = launch_db
        .find_launch_by_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("launch vanished after outcome"))?;
    assert_succeeded_row(&done, &stderr);

    let listed = launch_db
        .list_launches(&LaunchListFilter {
            backend: Some("herdr".to_owned()),
            open_only: true,
            ..LaunchListFilter::default()
        })
        .await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], done);

    launch_db
        .record_launch_closed(id, "kill", Some(true))
        .await?;
    launch_db.record_launch_closed(id, "reaped", None).await?;

    let closed = launch_db
        .find_launch_by_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("launch vanished after close"))?;
    assert!(closed.closed_at.is_some());
    assert_eq!(closed.close_reason.as_deref(), Some("kill"));
    assert_eq!(closed.close_process_exited, Some(true));
    assert_eq!(launch_db.count_open_launches("herdr").await?, 0);
    assert!(
        launch_db
            .list_launches(&LaunchListFilter {
                open_only: true,
                ..LaunchListFilter::default()
            })
            .await?
            .is_empty()
    );

    let second = launch_db
        .insert_launch(&full_launch(session, "relaunch brief"))
        .await?;
    launch_db
        .record_launch_opened(second, &LaunchSurfaceRefs::default())
        .await?;
    let found = launch_db
        .find_launch_by_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no launch for session"))?;
    assert_eq!(
        found.id, second,
        "the open relaunch wins over the closed original"
    );
    let ids: Vec<i64> = launch_db
        .list_launches(&LaunchListFilter::default())
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec![second, id], "newest launch first");

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn updates_on_unknown_launch_id_return_data_invariant() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let launch_db = handle.launch();

    let outcome = launch_db.record_launch_closed(-1, "kill", None).await;
    assert!(
        matches!(
            outcome,
            Err(mike::store::persistence::PersistError::DataInvariant(_))
        ),
        "closing an unknown launch must fail with DataInvariant: {outcome:?}"
    );

    Ok(())
}
