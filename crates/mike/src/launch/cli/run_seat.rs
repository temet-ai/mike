use super::runner::Runner;
use crate::launch::{OpenRequest, RunSeat, SplitLaunchArgs, Surface};
use crate::store::persistence::launch::NewRun;

pub(super) struct RunPlacement {
    pub(super) run_tab: Option<String>,
    pub(super) run: Option<RunSeat>,
}

pub(super) async fn resolve(runner: &Runner<'_>, split: &SplitLaunchArgs) -> RunPlacement {
    let Some(run_id) = &split.run else {
        return RunPlacement {
            run_tab: None,
            run: None,
        };
    };
    let row = runner.ledger.find_run(run_id).await;
    let run_tab = row.as_ref().and_then(|r| r.tab_ref.clone());
    let run = split.placement.is_none().then(|| RunSeat {
        run_id: run_id.clone(),
        workspace_ref: row.and_then(|r| r.workspace_ref),
        seat: split.seat.clone(),
        role: split.role.clone(),
    });
    RunPlacement { run_tab, run }
}

pub(super) async fn record_workspace(
    runner: &Runner<'_>,
    request: &OpenRequest,
    surface: &Surface,
) {
    let (Some(run), Some(workspace)) = (&request.run, &surface.workspace_ref) else {
        return;
    };
    if run.workspace_ref.as_deref() == Some(workspace.as_str()) {
        return;
    }
    runner
        .ledger
        .upsert_run(&NewRun {
            run_id: run.run_id.clone(),
            project_path: None,
            backend: surface.backend.as_str().to_string(),
            workspace_ref: Some(workspace.clone()),
            tab_ref: None,
        })
        .await;
}
