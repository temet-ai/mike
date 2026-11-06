use super::delivery::Delivery;
use super::open::Prepared;
use super::runner::Runner;
use crate::launch::{LaunchSpec, Placement, SplitLaunchArgs};
use crate::store::persistence::launch::{NewLaunch, NewRun};

pub(super) async fn record_start(
    runner: &Runner<'_>,
    p: &Prepared,
    spec: &LaunchSpec,
    delivery: &Delivery,
) -> Option<i64> {
    let backend = runner.backend.id().as_str().to_string();
    let project = p.repo_root.clone().unwrap_or_else(|| p.folder.clone());
    if let Some(run) = &p.split.run {
        let recorded = runner
            .ledger
            .upsert_run(&NewRun {
                run_id: run.clone(),
                project_path: Some(project.to_string_lossy().into_owned()),
                backend: backend.clone(),
                workspace_ref: None,
                tab_ref: None,
            })
            .await;
        if !recorded && runner.ledger.available() {
            return None;
        }
    }
    let mut new = NewLaunch::new(backend, p.folder.to_string_lossy().into_owned());
    new.session_uuid = spec.session;
    new.repo_root = p
        .repo_root
        .as_ref()
        .map(|r| r.to_string_lossy().into_owned());
    new.worktree = p.worktree;
    new.run_id = p.split.run.clone();
    new.seat = p.split.seat.clone();
    new.template_id = p.split.template_id;
    new.model = spec.model.clone();
    new.agent = spec.agent.clone();
    new.claude_args = delivery.args.clone();
    new.prompt_present = spec.prompt.is_some();
    new.brief_mode = spec.brief.as_ref().map(|b| b.mode.as_str().to_string());
    new.brief_path = spec
        .brief
        .as_ref()
        .and_then(|b| b.path.as_ref())
        .map(|path| path.to_string_lossy().into_owned());
    new.brief_note_uuid = spec.brief.as_ref().and_then(|b| b.note);
    new.brief_text = spec.prompt.clone();
    new.requested_placement = Some(p.split.placement.unwrap_or(Placement::Tab).as_str());
    new.split_direction = match p.split.placement {
        Some(Placement::Split(dir)) => Some(dir.as_str().to_string()),
        _ => None,
    };
    new.anchor_ref = p.split.anchor_terminal.clone();
    new.remote_control = p.split.remote_control;
    new.proxy_env = p.split.proxy_env && runner.proxy.is_some();
    new.identity = launch_identity_payload(&p.split);
    new.backend_reason = runner.backend_reason.clone();
    runner.ledger.insert_launch(&new).await
}

fn launch_identity_payload(split: &SplitLaunchArgs) -> Option<serde_json::Value> {
    let name = split.session_name.clone().or_else(|| split.seat.clone());
    if name.is_none()
        && split.role.is_none()
        && split.instructions.is_none()
        && split.goal.is_none()
    {
        return None;
    }
    Some(serde_json::json!({
        "name": name,
        "role": split.role,
        "instructions": split.instructions,
        "goal": split.goal,
    }))
}
