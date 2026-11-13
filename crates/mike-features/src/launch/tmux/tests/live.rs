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
use crate::launch::request::agent_name;
use crate::launch::tmux::TmuxBackend;

const FAKE_CLAUDE: &str = "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$MARKER_FILE\"\nsleep 30\n";

#[tokio::test]
async fn live_open_close_round_trip_against_real_tmux_when_opted_in() -> anyhow::Result<()> {
    if std::env::var("MIKE_LIVE_TMUX").as_deref() != Ok("1") {
        return Ok(());
    }
    let backend = TmuxBackend::new();
    let probe = backend.probe().await;
    assert!(probe.available, "{probe:?}");
    let validation = backend.validate().await;
    assert!(validation.passed, "{validation:?}");

    let bin_dir = tempfile::tempdir()?;
    let script = bin_dir.path().join("claude");
    std::fs::write(&script, FAKE_CLAUDE)?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    let marker = tempfile::NamedTempFile::new()?;
    let workdir = tempfile::tempdir()?;

    let mut env = BTreeMap::new();
    env.insert(
        "PATH".to_string(),
        format!(
            "{}:{}",
            bin_dir.path().display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    env.insert(
        "MARKER_FILE".to_string(),
        marker.path().display().to_string(),
    );

    let session = Uuid::new_v4();
    let req = OpenRequest {
        folder: workdir.path().to_path_buf(),
        placement: Placement::Window,
        env,
        title: "t".into(),
        run_tab: None,
        run: None,
        program: "claude".into(),
        args: vec!["--session-id".into(), session.to_string()],
        name: agent_name(session),
        prompt: Some("hello from the live test".into()),
        prompt_wait_ms: 2_000,
    };

    let surface = backend.open(&req).await?;
    assert_eq!(surface.session_uuid, Some(session));
    assert!(
        surface.pid.is_some(),
        "a real claude child should have a pid"
    );
    assert_eq!(surface.delivered, Some(true));

    let marker_text = poll_marker(marker.path(), Duration::from_secs(3)).await;
    assert!(
        marker_text.contains("hello from the live test"),
        "marker file: {marker_text:?}"
    );

    let found = backend.find_by_session(session).await?;
    assert!(
        found.is_some(),
        "find_by_session must resolve the live surface"
    );

    let outcome = backend.close(&surface, Duration::from_secs(3)).await?;
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
