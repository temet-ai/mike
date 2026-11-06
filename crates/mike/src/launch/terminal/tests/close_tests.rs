use std::collections::BTreeMap;
use std::time::Duration;

use uuid::Uuid;

use crate::launch::backend::{CloseOutcome, OpenRequest, Placement, Surface, TerminalBackend};
use crate::launch::terminal::XdgTerminalBackend;

use super::{FakeTerminalHost, fast_timeouts};

fn backend(host: &FakeTerminalHost) -> XdgTerminalBackend {
    XdgTerminalBackend::with_binaries_and_timeouts(host.xdg_bin(), host.hypr_bin(), fast_timeouts())
}

async fn open_long_lived(
    backend: &XdgTerminalBackend,
    folder: &std::path::Path,
    session: Uuid,
) -> anyhow::Result<Surface> {
    let req = OpenRequest {
        folder: folder.to_path_buf(),
        placement: Placement::Window,
        env: BTreeMap::new(),
        title: "t".into(),
        run_tab: None,
        run: None,
        program: "sh".into(),
        args: vec![
            "-c".into(),
            "sleep 2".into(),
            "--session-id".into(),
            session.to_string(),
        ],
        name: "cc-close".into(),
        prompt: None,
        prompt_wait_ms: 0,
    };
    Ok(backend.open(&req).await?)
}

#[tokio::test]
async fn close_a_known_window_reports_the_process_gone() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let b = backend(&host);
    let session = Uuid::new_v4();
    let surface = open_long_lived(&b, folder.path(), session).await?;
    let outcome = b.close(&surface, Duration::from_secs(3)).await?;
    assert_eq!(
        outcome,
        CloseOutcome::Closed {
            process_exited: true
        }
    );
    Ok(())
}

#[tokio::test]
async fn close_is_idempotent_a_second_close_is_not_found() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let b = backend(&host);
    let session = Uuid::new_v4();
    let surface = open_long_lived(&b, folder.path(), session).await?;
    let first = b.close(&surface, Duration::from_secs(3)).await?;
    assert!(matches!(first, CloseOutcome::Closed { .. }));
    let second = b.close(&surface, Duration::from_millis(100)).await?;
    assert_eq!(second, CloseOutcome::NotFound);
    Ok(())
}

#[tokio::test]
async fn close_unknown_pid_is_not_found() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let b = backend(&host);
    let surface = Surface {
        backend: crate::launch::BackendId::Terminal,
        workspace_ref: None,
        tab_ref: None,
        surface_ref: "999999999".into(),
        pid: None,
        session_uuid: None,
        agent_state: None,
        cwd: None,
        actual_placement: "window".into(),
        reused: false,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    };
    let outcome = b.close(&surface, Duration::from_millis(100)).await?;
    assert_eq!(outcome, CloseOutcome::NotFound);
    Ok(())
}

#[tokio::test]
async fn close_wrong_session_is_a_mismatch_and_leaves_the_window_open() -> anyhow::Result<()> {
    let host = FakeTerminalHost::new();
    let folder = tempfile::tempdir()?;
    let b = backend(&host);
    let opened_session = Uuid::new_v4();
    let mut surface = open_long_lived(&b, folder.path(), opened_session).await?;
    surface.session_uuid = Some(Uuid::new_v4());

    let outcome = b.close(&surface, Duration::from_millis(200)).await?;
    assert!(
        matches!(outcome, CloseOutcome::Mismatch { .. }),
        "{outcome:?}"
    );
    let pid: u32 = surface.surface_ref.parse()?;
    assert!(
        crate::launch::process::alive(pid),
        "a mismatched close must not touch the window"
    );
    Ok(())
}
