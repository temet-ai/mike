#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use uuid::Uuid;

use crate::launch::backend::{CloseOutcome, OpenRequest, Placement, TerminalBackend};
use crate::launch::terminal::XdgTerminalBackend;

const FAKE_CLAUDE: &str = "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$MARKER_FILE\"\nsleep 30\n";

#[tokio::test]
async fn live_open_close_round_trip_against_real_xdg_terminal_exec_and_hyprctl_when_opted_in()
-> anyhow::Result<()> {
    if std::env::var("MIKE_LIVE_TERMINAL").as_deref() != Ok("1") {
        return Ok(());
    }
    let backend = XdgTerminalBackend::new();
    let probe = backend.probe().await;
    assert!(probe.available, "{probe:?}");
    let validation = backend.validate().await;
    assert!(validation.passed, "{validation:?}");

    let bin_dir = tempfile::tempdir()?;
    let claude = bin_dir.path().join("claude");
    std::fs::write(&claude, FAKE_CLAUDE)?;
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755))?;
    let marker = tempfile::NamedTempFile::new()?;
    let workdir = tempfile::tempdir()?;

    let mut env = BTreeMap::new();
    env.insert(
        "MARKER_FILE".to_string(),
        marker.path().display().to_string(),
    );

    let session = Uuid::new_v4();
    let req = OpenRequest {
        folder: workdir.path().to_path_buf(),
        placement: Placement::Window,
        env,
        title: "mike live terminal test".into(),
        run_tab: None,
        run: None,
        program: claude.to_string_lossy().into_owned(),
        args: vec!["--session-id".into(), session.to_string()],
        name: "cc-live".into(),
        prompt: None,
        prompt_wait_ms: 0,
    };

    let surface = backend.open(&req).await?;
    assert_eq!(surface.session_uuid, Some(session));
    assert!(
        surface.pid.is_some(),
        "a real claude child should have a pid"
    );

    let marker_text = poll_marker(marker.path(), Duration::from_secs(5)).await;
    assert!(
        marker_text.contains(&session.to_string()),
        "marker file: {marker_text:?}"
    );

    let found = backend.find_by_session(session).await?;
    assert!(
        found.is_some(),
        "find_by_session must resolve the live surface"
    );

    let outcome = backend.close(&surface, Duration::from_secs(5)).await?;
    assert!(
        matches!(outcome, CloseOutcome::Closed { .. }),
        "{outcome:?}"
    );
    let repeat = backend.close(&surface, Duration::from_millis(100)).await?;
    assert_eq!(
        repeat,
        CloseOutcome::NotFound,
        "a second close must be a no-op"
    );
    Ok(())
}

async fn poll_marker(path: &std::path::Path, budget: Duration) -> String {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.is_empty()
        {
            return text;
        }
        if tokio::time::Instant::now() >= deadline {
            return std::fs::read_to_string(path).unwrap_or_default();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
