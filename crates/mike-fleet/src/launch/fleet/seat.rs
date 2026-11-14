use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::bundle;
use super::definition::{FleetDefinition, merge};
use super::environment::contract_path;
use super::events::{EventPayload, NewFleetEvent, SeatLaunchFailed, SeatLaunchRequested};
use super::fork_seat;
use super::plan::SeatPlan;
use super::seat_settings;
use crate::base_state::BaseState;
use crate::launch::{ServerOpenRequest, open_for_server};
use crate::store::persistence::PersistHandle;

const OPEN_WAIT: Duration = Duration::from_secs(150);
const OPEN_POLL: Duration = Duration::from_millis(500);
pub const SAVE_ATTEMPTS: usize = 5;
pub const SAVE_RETRY: Duration = Duration::from_millis(500);

pub struct Begun {
    pub started: Result<(), String>,
    pub forked_from: Option<String>,
}

pub fn claude_args(def: &FleetDefinition, seat: &SeatPlan) -> Vec<String> {
    let mut args = vec!["--model".to_string(), seat.model.clone()];
    if let Some(level) = effort(seat) {
        args.push("--effort".to_string());
        args.push(level);
    }
    if !seat.agent.trim().is_empty() {
        args.push("--agent".to_string());
        args.push(seat.agent.clone());
    }
    args.push("--settings".to_string());
    args.push(seat_settings::for_seat(def, &seat.seat).to_string());
    if let Some(file) = seat.contract.as_deref().filter(|f| !f.trim().is_empty()) {
        args.push("--append-system-prompt-file".to_string());
        let state = bundle::default_state_dir(&bundle::home());
        let path = bundle::resolve_contract(&def.repo, &state, file)
            .unwrap_or_else(|| contract_path(&def.repo, file));
        args.push(path.to_string_lossy().into_owned());
    }
    args
}

fn effort(seat: &SeatPlan) -> Option<String> {
    let named = seat
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    match named {
        Some(level) => Some(level.to_string()),
        None if seat.model.contains("haiku") => Some("medium".to_string()),
        None => None,
    }
}

pub async fn begin(
    state: &BaseState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    run_id: &str,
    seat: &SeatPlan,
) -> Begun {
    if let Err(problem) = admitted_run(persist, run_id).await {
        return Begun {
            started: Err(problem),
            forked_from: None,
        };
    }
    if !seat.runtime.controls_through_terminal() {
        return Begun {
            started: begin_codex(persist, def, run_id, seat).await,
            forked_from: None,
        };
    }
    if let Err(problem) = seat_settings::prepare(def) {
        return Begun {
            started: Err(problem),
            forked_from: None,
        };
    }
    let args = claude_args(def, seat);
    let template = fork_seat::template_for(persist, run_id, seat, &args).await;
    if let Some(row) = &template
        && let Some(refusal) = fork_seat::refuse_over_limit(persist, def, run_id, row).await
    {
        return Begun {
            started: Err(refusal),
            forked_from: None,
        };
    }
    let forked_from = template.as_ref().map(|t| t.name.clone());
    let template_id = template.as_ref().map(|t| t.id);
    let args = match &template {
        Some(row) => fork_seat::fork_args(row, seat.session, args),
        None => fork_seat::cold_args(seat.session, args),
    };
    let request = ServerOpenRequest {
        folder: def.repo.clone(),
        claude_args: args,
        prompt: Some(super::report_proof::starting_prompt(persist, def, run_id, seat).await),
        run: Some(run_id.to_string()),
        seat: Some(seat.seat.clone()),
        session_name: Some(seat_name(&seat.seat, seat.session)),
        role: Some(seat.role.clone()),
        instructions: Some(seat.instructions.clone()),
        goal: Some(seat.goal.clone()),
        template_id,
        worktree_ok: super::environment::repo_is_linked_worktree(&def.repo).await,
        trust: false,
        proxy_env: true,
        remote_control: true,
        channels: true,
        backend: None,
        caller: Default::default(),
    };
    let started = open_for_server(state.clone(), request)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string());
    record_launch(persist, run_id, seat, &started).await;
    Begun {
        started,
        forked_from,
    }
}

async fn begin_codex(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run_id: &str,
    seat: &SeatPlan,
) -> Result<(), String> {
    let state = bundle::default_state_dir(&bundle::home());
    let scratch = seat_settings::scratch_dir(&state, &def.repo);
    let started = crate::launch::codex::begin_planned(
        persist,
        def,
        run_id,
        seat,
        &scratch,
        &seat_name(&seat.seat, seat.session),
    )
    .await;
    record_launch(persist, run_id, seat, &started).await;
    started
}

async fn record_launch(
    persist: &PersistHandle,
    run_id: &str,
    seat: &SeatPlan,
    started: &Result<(), String>,
) {
    let launch_id = persist
        .launch()
        .find_launch_by_session(seat.session)
        .await
        .ok()
        .flatten()
        .map_or(0, |row| row.id);
    let payload = match started {
        Ok(()) => EventPayload::SeatLaunchRequested(SeatLaunchRequested { launch_id }),
        Err(reason) => EventPayload::SeatLaunchFailed(SeatLaunchFailed {
            launch_id,
            reason: reason.clone(),
        }),
    };
    let event = NewFleetEvent {
        run_id: run_id.to_string(),
        task: seat.task.clone(),
        session_uuid: Some(seat.session),
        payload,
    };
    if let Err(err) = persist.fleet().append_fleet_event(&event).await {
        tracing::warn!(error = %err, run = run_id, "fleet: a seat launch event could not be recorded");
    }
}

async fn admitted_run(persist: &PersistHandle, run: &str) -> Result<(), String> {
    let recorded = persist
        .fleet()
        .fleet_run_admission(run)
        .await
        .map_err(|error| error.to_string())?;
    if recorded.is_some_and(|row| row.admits_work()) {
        return Ok(());
    }
    Err(format!(
        "seat launch refused: exact fleet run {run} is no longer starting or running"
    ))
}

pub fn seat_name(seat: &str, session: Uuid) -> String {
    let short: String = session.simple().to_string().chars().take(8).collect();
    format!("{seat}-{short}")
}

pub async fn finish(persist: &PersistHandle, seat: &SeatPlan, begun: Begun) -> Value {
    let opened = match begun.started {
        Ok(()) => wait_until_open(persist, seat.session).await,
        Err(problem) => Err(problem),
    };
    json!({
        "seat": seat.seat,
        "task": seat.task,
        "session": seat.session,
        "forked_from": begun.forked_from,
        "opened": opened.is_ok(),
        "problem": opened.err(),
    })
}

pub async fn launch(
    state: &BaseState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    run_id: &str,
    seat: &SeatPlan,
) -> Value {
    let begun = begin(state, persist, def, run_id, seat).await;
    finish(persist, seat, begun).await
}

pub fn opened(report: &Value) -> bool {
    report["opened"] == json!(true)
}

pub fn problem(report: &Value) -> String {
    report["problem"]
        .as_str()
        .unwrap_or("the seat did not open")
        .to_string()
}

async fn wait_until_open(persist: &PersistHandle, session: Uuid) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + OPEN_WAIT;
    loop {
        let row = persist
            .launch()
            .find_launch_by_session(session)
            .await
            .map_err(|e| e.to_string())?;
        match row {
            Some(row) if row.opened => return Ok(()),
            Some(row)
                if row.result == "failed"
                    || row.reason.as_deref() == Some("fleet_open_unconfirmed") =>
            {
                return Err(row
                    .stderr_text
                    .or(row.reason)
                    .unwrap_or_else(|| "the launch failed".to_string()));
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "the seat's pane did not open within {} seconds",
                OPEN_WAIT.as_secs()
            ));
        }
        tokio::time::sleep(OPEN_POLL).await;
    }
}

pub async fn patch_definition(persist: &PersistHandle, fleet: Uuid, patch: &Value) -> bool {
    patch_definition_if(persist, fleet, patch, |_| true).await
}

pub async fn patch_definition_if(
    persist: &PersistHandle,
    fleet: Uuid,
    patch: &Value,
    still: impl Fn(&Value) -> bool,
) -> bool {
    for _ in 0..SAVE_ATTEMPTS {
        match save_once(persist, fleet, patch, &still).await {
            Saved::Yes => return true,
            Saved::Closed => return false,
            Saved::Raced => {}
            Saved::Failed(err) => save_failed(fleet, &err).await,
        }
    }
    gave_up(fleet);
    false
}

async fn save_failed(fleet: Uuid, err: &str) {
    tracing::warn!(%fleet, error = %err, "mike could not save a change to the fleet definition; trying again");
    tokio::time::sleep(SAVE_RETRY).await;
}

fn gave_up(fleet: Uuid) {
    tracing::warn!(%fleet, "mike gave up saving a change to the fleet definition");
}

enum Saved {
    Yes,
    Closed,
    Raced,
    Failed(String),
}

async fn save_once(
    persist: &PersistHandle,
    fleet: Uuid,
    patch: &Value,
    still: &impl Fn(&Value) -> bool,
) -> Saved {
    let row = match persist.fleet().find_fleet(&fleet.to_string()).await {
        Ok(Some(row)) if matches!(row.state.as_str(), "starting" | "running") => row,
        Ok(_) => return Saved::Closed,
        Err(err) => return Saved::Failed(err.to_string()),
    };
    if !still(&row.definition) {
        return Saved::Closed;
    }
    let mut definition = row.definition.clone();
    merge(&mut definition, patch);
    match persist
        .fleet()
        .save_fleet_definition(fleet, &definition, &row.missing, row.updated_at)
        .await
    {
        Ok(Some(_)) => Saved::Yes,
        Ok(None) => Saved::Raced,
        Err(err) => Saved::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> SeatPlan {
        SeatPlan {
            runtime: crate::runtime::RuntimeKind::Claude,
            seat: "rust-developer-T3".into(),
            role: "rust-developer".into(),
            agent: "rust-developer".into(),
            model: "sonnet".into(),
            effort: None,
            contract: None,
            session: Uuid::nil(),
            task: None,
            discipline: Some("rust-developer".into()),
            prompt: String::new(),
            instructions: String::new(),
            goal: String::new(),
        }
    }

    fn effort_of(model: &str, effort: Option<&str>) -> Option<String> {
        let seat = SeatPlan {
            model: model.into(),
            effort: effort.map(Into::into),
            ..plan()
        };
        let args = claude_args(&FleetDefinition::default(), &seat);
        let at = args.iter().position(|a| a == "--effort")?;
        args.get(at + 1).cloned()
    }

    #[test]
    fn a_haiku_seat_runs_at_medium_effort_unless_the_definition_names_one() {
        assert_eq!(effort_of("haiku", None).as_deref(), Some("medium"));
        assert_eq!(
            effort_of("claude-haiku-5-5", None).as_deref(),
            Some("medium")
        );
        assert_eq!(effort_of("haiku", Some("high")).as_deref(), Some("high"));
        assert_eq!(effort_of("sonnet", Some("low")).as_deref(), Some("low"));
        assert_eq!(effort_of("sonnet", None), None);
        assert_eq!(effort_of("opus", Some(" ")), None);
    }

    #[test]
    fn a_seat_is_launched_with_the_definitions_seat_settings() {
        let settings = json!({"env": {"EXTRA": "1"}});
        let def = FleetDefinition {
            seat_settings: Some(settings),
            ..FleetDefinition::default()
        };
        let args = claude_args(&def, &plan());
        let at = args.iter().position(|a| a == "--settings");
        let launched: Value = at
            .and_then(|i| args.get(i + 1))
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        assert_eq!(launched["enabledPlugins"], Value::Null);
        assert_eq!(launched["env"]["EXTRA"], json!("1"));
    }

    fn facts() -> super::super::plan::FleetFacts {
        super::super::plan::FleetFacts {
            fleet: Uuid::nil().to_string(),
            slug: "slug".into(),
            run_id: "fleet-slug-00000000".into(),
            seats: super::super::plan::ControlSeats::answering(None, None),
            paths: super::super::bundle::FleetPaths {
                rules: "/r/rules.md".into(),
                checks: "/r/checks.sh".into(),
                project: None,
            },
        }
    }

    fn definition_with_settings() -> FleetDefinition {
        use super::super::definition::{TaskDefinition, WorkerDefinition};
        let mut def = FleetDefinition {
            seat_settings: Some(json!({"enabledPlugins": {"x@y": false}})),
            ..FleetDefinition::default()
        };
        def.workers.insert(
            "rust-developer".into(),
            WorkerDefinition {
                runtime: crate::runtime::RuntimeKind::Claude,
                model: "sonnet".into(),
                effort: None,
                agent: "rust-developer".into(),
                contract: None,
            },
        );
        def.tasks.insert(
            "T1".into(),
            TaskDefinition {
                discipline: "rust-developer".into(),
                ..TaskDefinition::default()
            },
        );
        def
    }

    #[test]
    fn a_template_and_its_fork_are_launched_with_the_same_arguments() {
        let def = definition_with_settings();
        let facts = facts();
        let template = super::super::plan::template(&def, &facts, "rust-developer");
        let worker = super::super::plan::worker(&def, &facts, "T1", Uuid::from_u128(5));
        let (Some(template), Some(worker)) = (template, worker) else {
            panic!("the definition must yield a template and a worker plan");
        };
        assert_eq!(template.seat, "template-rust-developer");
        assert_ne!(template.seat, worker.seat);
        let template_args = claude_args(&def, &template);
        let worker_args = claude_args(&def, &worker);
        assert!(template_args.iter().any(|a| a == "--settings"));
        assert_eq!(template_args, worker_args);
        let row = crate::store::persistence::launch::TemplateRow {
            id: 1,
            name: "n".into(),
            session_uuid: Uuid::nil(),
            folder: "/r".into(),
            backend: "herdr".into(),
            model: None,
            agent: None,
            claude_args: json!(template_args),
            created_at: chrono::Utc::now(),
            superseded_by: None,
            closed_at: None,
        };
        assert!(fork_seat::matches(&row, &worker_args));
    }

    #[test]
    fn without_seat_settings_a_seat_is_still_launched_with_the_default_settings()
    -> anyhow::Result<()> {
        let args = claude_args(&FleetDefinition::default(), &plan());
        let at = args.iter().position(|a| a == "--settings");
        let text = at
            .and_then(|i| args.get(i + 1))
            .ok_or_else(|| anyhow::anyhow!("no --settings value in {args:?}"))?;
        let settings: Value = serde_json::from_str(text)?;
        assert_eq!(settings["enabledPlugins"], Value::Null);
        assert!(
            settings["env"]["TMPDIR"]
                .as_str()
                .is_some_and(|t| t.contains("/seats/") && t.ends_with("/tmp"))
        );
        Ok(())
    }
}
