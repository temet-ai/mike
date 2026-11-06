use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv};
use super::run_layout;
use crate::launch::backend::{BackendId, PaneRequest, Surface};
use crate::launch::error::LaunchError;

pub(super) async fn open(cli: &HerdrCli, req: &PaneRequest) -> Result<Surface, LaunchError> {
    let folder = req.folder.to_string_lossy().into_owned();
    let placed = run_layout::place(cli, &req.run, &folder, &[]).await?;
    let run = argv(&["pane", "run", &placed.pane.pane, &req.command]);
    cli.run_json(&run, DEFAULT_TIMEOUT).await?;
    Ok(Surface {
        backend: BackendId::Herdr,
        workspace_ref: Some(placed.pane.workspace),
        tab_ref: Some(placed.pane.tab),
        surface_ref: placed.pane.pane,
        pid: None,
        session_uuid: None,
        agent_state: None,
        cwd: Some(folder),
        actual_placement: placed.placement,
        reused: !placed.created_workspace,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    })
}
