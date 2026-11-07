use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use uuid::Uuid;

use crate::launch::backend::{OpenRequest, Placement, TerminalBackend};
use crate::launch::terminal::XdgTerminalBackend;

use super::{FakeTerminalHost, fast_timeouts};

fn backend(host: &FakeTerminalHost) -> XdgTerminalBackend {
    XdgTerminalBackend::with_binaries_and_timeouts(host.xdg_bin(), host.hypr_bin(), fast_timeouts())
}

fn request(folder: &std::path::Path, extra_args: Vec<String>, name: &str) -> OpenRequest {
    let mut args = vec!["-c".to_string(), "sleep 0.5".to_string()];
    args.extend(extra_args);
    OpenRequest {
        folder: folder.to_path_buf(),
        placement: Placement::Window,
        env: BTreeMap::new(),
        title: "t".into(),
        run_tab: None,
        run: None,
        program: "sh".into(),
        args,
        name: name.into(),
        prompt: None,
        prompt_wait_ms: 0,
    }
}

#[tokio::test]
async fn open_tags_the_window_with_the_full_session_uuid() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let req = request(
        folder.path(),
        vec!["--session-id".into(), session.to_string()],
        "cc-x",
    );
    let surface = backend(&host).open(&req).await?;
    assert_eq!(surface.session_uuid, Some(session));
    let pid: u32 = surface.surface_ref.parse()?;
    assert!(
        host.windows()
            .iter()
            .any(|(p, tag)| *p == pid && *tag == format!("mike-{session}"))
    );
    assert_eq!(surface.actual_placement, "window");
    assert_eq!(
        surface.cwd.as_deref(),
        Some(folder.path().to_string_lossy().as_ref())
    );
    Ok(())
}

#[tokio::test]
async fn open_without_a_session_id_tags_with_the_agent_name() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let req = request(folder.path(), vec![], "cc-abcd1234");
    let surface = backend(&host).open(&req).await?;
    assert_eq!(surface.session_uuid, None);
    let pid: u32 = surface.surface_ref.parse()?;
    assert!(
        host.windows()
            .iter()
            .any(|(p, tag)| *p == pid && tag == "mike-cc-abcd1234")
    );
    Ok(())
}

#[tokio::test]
async fn open_reports_a_non_window_placement_request_honestly() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let mut req = request(folder.path(), vec![], "cc-y");
    req.placement = Placement::Tab;
    let surface = backend(&host).open(&req).await?;
    assert!(
        surface
            .actual_placement
            .starts_with("window (requested tab"),
        "{}",
        surface.actual_placement
    );
    Ok(())
}

#[tokio::test]
async fn open_resolves_the_claude_child_pid_one_level_below_the_window() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let bin_dir = tempfile::tempdir()?;
    let real_dir = tempfile::tempdir()?;
    let real = real_dir.path().join("claude");
    std::fs::copy("/bin/sleep", &real)?;
    if cfg!(target_os = "macos") {
        let signed = std::process::Command::new("codesign")
            .args(["--sign", "-", "--force"])
            .arg(&real)
            .output()?;
        anyhow::ensure!(signed.status.success(), "{signed:?}");
    }
    let claude = bin_dir.path().join("claude");
    std::fs::write(
        &claude,
        format!("#!/bin/sh\nexec '{}' 5\n", real.display()),
    )?;
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755))?;

    let mut req = request(folder.path(), vec![], "cc-z");
    req.program = claude.to_string_lossy().into_owned();
    req.args = vec![];
    let unloaded_host_budget = crate::launch::terminal::TerminalTimeouts {
        window: Duration::from_secs(4),
        child_pid: Duration::from_secs(4),
        ..fast_timeouts()
    };
    let surface = XdgTerminalBackend::with_binaries_and_timeouts(
        host.xdg_bin(),
        host.hypr_bin(),
        unloaded_host_budget,
    )
    .open(&req)
    .await?;
    let window_pid: u32 = surface.surface_ref.parse()?;
    assert!(
        surface.pid.is_some_and(|pid| pid != window_pid),
        "the claude pid must be a real child of the window, not the window itself: {surface:?}"
    );
    Ok(())
}

#[tokio::test]
async fn open_propagates_environment_through_the_exec_chain() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let marker = folder.path().join("env-marker");
    let mut req = request(folder.path(), vec![], "cc-env");
    req.env
        .insert("MIKE_OPEN_TEST_VAR".to_string(), "seen".to_string());
    req.args = vec![
        "-c".into(),
        format!(
            "printf '%s' \"$MIKE_OPEN_TEST_VAR\" > {}",
            marker.to_string_lossy()
        ),
    ];
    let _surface = backend(&host).open(&req).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        content, "seen",
        "the fake xdg-terminal-exec must pass its own env through"
    );
    Ok(())
}
