use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{FleetFacts, SeatPlan, reference, template, template_name, warm_disciplines};
use super::seat::{self, Begun};
use crate::config::AppState;
use crate::launch::{BackendId, TerminalBackend, backend_for, kill_for_server};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{LaunchRow, NewTemplate};

const LOAD_WAIT: Duration = Duration::from_secs(480);
const LOAD_POLL: Duration = Duration::from_secs(3);
const QUIET_POLLS: u32 = 2;
const PENDING: &str = "pending";
const FAILED: &str = "failed";
const WORKING: &str = "working";
const AT_REST: [&str; 2] = ["idle", "done"];

pub struct Warming {
    discipline: String,
    plan: SeatPlan,
    begun: Begun,
}

pub async fn begin_all(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
) -> Vec<Warming> {
    let mut warming = Vec::new();
    for discipline in warm_disciplines(def) {
        let Some(mut plan) = template(def, facts, &discipline) else {
            continue;
        };
        plan.prompt = carried_prompt(persist, def, facts, &discipline, &plan).await;
        let begun = seat::begin(state, persist, def, &facts.run_id, &plan).await;
        warming.push(Warming {
            discipline,
            plan,
            begun,
        });
    }
    warming
}

async fn carried_prompt(
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    discipline: &str,
    plan: &SeatPlan,
) -> String {
    let agent = def
        .workers
        .get(discipline)
        .map_or(discipline, |w| w.agent.as_str());
    let lines =
        super::carry::warm_lines(persist, &facts.fleet, &facts.run_id, &[discipline, agent]).await;
    super::carry::with_carry(&plan.prompt, &lines)
}

pub async fn finish_all(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    warming: Vec<Warming>,
) -> Vec<Value> {
    let loads = warming
        .into_iter()
        .map(|w| finish_one(state, persist, def, facts, w));
    futures::future::join_all(loads).await
}

async fn finish_one(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    warming: Warming,
) -> Value {
    let Warming {
        discipline,
        plan,
        begun,
    } = warming;
    let mut report = seat::finish(persist, &plan, begun).await;
    if !seat::opened(&report) {
        report["loaded"] = json!(false);
        return report;
    }
    let loaded = match loaded(state, persist, plan.session).await {
        Ok(()) => record(state, persist, def, (&facts.run_id, &discipline), &plan).await,
        Err(problem) => Err(problem),
    };
    close(state, plan.session).await;
    report["loaded"] = json!(loaded.is_ok());
    report["problem"] = json!(loaded.err());
    report
}

async fn loaded(state: &AppState, persist: &PersistHandle, session: Uuid) -> Result<(), String> {
    let backend = state
        .config
        .launch
        .backend
        .parse::<BackendId>()
        .ok()
        .and_then(backend_for)
        .ok_or_else(|| "no terminal backend to read the template's state from".to_string())?;
    let deadline = tokio::time::Instant::now() + LOAD_WAIT;
    let mut progress = Progress::default();
    loop {
        let row = persist
            .find_launch_by_session(session)
            .await
            .map_err(|e| e.to_string())?;
        let agent = agent_state(backend.as_ref(), session).await;
        let in_flight = state.in_flight.is_session_busy(&session.to_string());
        match progress.step(row.as_ref().map(Launch::of), agent.as_deref(), in_flight) {
            Step::Loaded => return Ok(()),
            Step::Failed(problem) => return Err(problem),
            Step::Wait => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "the template had not finished loading after {} seconds; its workers start cold",
                LOAD_WAIT.as_secs()
            ));
        }
        tokio::time::sleep(LOAD_POLL).await;
    }
}

async fn agent_state(backend: &dyn TerminalBackend, session: Uuid) -> Option<String> {
    backend
        .find_by_session(session)
        .await
        .ok()
        .flatten()
        .and_then(|surface| surface.agent_state)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Progress {
    worked: bool,
    quiet: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Wait,
    Loaded,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launch<'a> {
    pub result: &'a str,
    pub confirmed: bool,
    pub reason: Option<&'a str>,
}

impl<'a> Launch<'a> {
    fn of(row: &'a LaunchRow) -> Self {
        Self {
            result: &row.result,
            confirmed: row.confirmed,
            reason: row.reason.as_deref(),
        }
    }
}

impl Progress {
    pub fn step(
        &mut self,
        launch: Option<Launch<'_>>,
        agent: Option<&str>,
        request_in_flight: bool,
    ) -> Step {
        let Some(launch) = launch else {
            return Step::Wait;
        };
        if launch.result == FAILED {
            return Step::Failed(
                launch
                    .reason
                    .unwrap_or("the template's launch failed")
                    .to_string(),
            );
        }
        if launch.confirmed || agent == Some(WORKING) {
            self.worked = true;
        }
        if launch.result == PENDING || !self.worked || request_in_flight {
            self.quiet = 0;
            return Step::Wait;
        }
        if agent.is_some_and(|state| AT_REST.contains(&state)) {
            self.quiet += 1;
        } else {
            self.quiet = 0;
        }
        if self.quiet >= QUIET_POLLS {
            Step::Loaded
        } else {
            Step::Wait
        }
    }
}

async fn record(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    (run_id, discipline): (&str, &str),
    plan: &SeatPlan,
) -> Result<(), String> {
    let new = NewTemplate {
        name: template_name(run_id, discipline),
        session_uuid: plan.session,
        folder: def.repo.clone(),
        backend: state.config.launch.backend.clone(),
        model: Some(plan.model.clone()),
        agent: Some(plan.agent.clone()),
        claude_args: seat::claude_args(def, plan),
    };
    persist
        .insert_template(&new)
        .await
        .map(|_| ())
        .map_err(|e| format!("the template loaded but could not be recorded: {e}"))
}

async fn close(state: &AppState, session: Uuid) {
    if let Err(err) = kill_for_server(state, session).await {
        tracing::warn!(session_id = %session, error = %err, "mike could not close a template's pane after it loaded");
    }
}

pub fn loaded_disciplines(reports: &[Value]) -> Vec<String> {
    reports
        .iter()
        .filter(|r| r["loaded"] == json!(true))
        .filter_map(|r| r["seat"].as_str())
        .filter_map(|seat| seat.strip_prefix(super::plan::TEMPLATE_SEAT_PREFIX))
        .map(str::to_string)
        .collect()
}

pub fn reference_discipline(def: &FleetDefinition, loaded: &[String]) -> Option<String> {
    if !def.reference.enabled {
        return None;
    }
    match def.reference.discipline.as_deref() {
        Some(wanted) => loaded.iter().find(|d| *d == wanted).cloned(),
        None => loaded.first().cloned(),
    }
}

pub async fn start_reference(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    reports: &[Value],
) -> Option<Value> {
    let discipline = reference_discipline(def, &loaded_disciplines(reports))?;
    let plan = reference(def, facts, &discipline, Uuid::new_v4())?;
    let report = seat::launch(state, persist, def, &facts.run_id, &plan).await;
    if seat::opened(&report) {
        let fleet = facts.fleet.parse::<Uuid>().ok()?;
        let patch = json!({"reference": {"session": plan.session, "discipline": discipline}});
        seat::patch_definition(persist, fleet, &patch).await;
    } else {
        tracing::warn!(problem = %seat::problem(&report), "mike could not start the run's reference seat");
    }
    Some(report)
}

pub async fn succeed_reference(
    state: &AppState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    old: Uuid,
) -> Option<Uuid> {
    let discipline = def
        .reference
        .discipline
        .clone()
        .or_else(|| reference_discipline(def, &warm_disciplines(def)))?;
    let plan = reference(def, facts, &discipline, Uuid::new_v4())?;
    let report = seat::launch(state, persist, def, &facts.run_id, &plan).await;
    if !seat::opened(&report) {
        tracing::warn!(problem = %seat::problem(&report), "mike could not start the reference seat's successor");
        return None;
    }
    let fleet = facts.fleet.parse::<Uuid>().ok()?;
    let patch = json!({"reference": {"session": plan.session, "discipline": discipline}});
    seat::patch_definition(persist, fleet, &patch).await;
    close(state, old).await;
    Some(plan.session)
}

pub async fn retire(persist: &PersistHandle, def: &FleetDefinition, run_id: &str) {
    for discipline in def.workers.keys() {
        if let Err(err) = persist
            .close_template(&template_name(run_id, discipline))
            .await
        {
            tracing::warn!(error = %err, run = %run_id, %discipline, "mike could not retire a fleet's template");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Launch, Progress, Step};

    fn launch(result: &'static str, confirmed: bool) -> Option<Launch<'static>> {
        Some(Launch {
            result,
            confirmed,
            reason: None,
        })
    }

    #[test]
    fn a_template_is_loaded_once_its_prompt_ran_and_its_pane_stays_at_rest() {
        let mut progress = Progress::default();
        assert_eq!(progress.step(None, None, false), Step::Wait);
        assert_eq!(
            progress.step(launch("pending", false), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("working"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Loaded
        );
    }

    #[test]
    fn a_pane_that_goes_back_to_work_is_not_loaded_yet() {
        let mut progress = Progress::default();
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("working"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("done"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("done"), false),
            Step::Loaded
        );
    }

    #[test]
    fn an_unconfirmed_prompt_counts_only_after_the_pane_was_seen_working() {
        let mut progress = Progress::default();
        assert_eq!(
            progress.step(launch("unconfirmed", false), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("unconfirmed", false), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("unconfirmed", false), Some("working"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("unconfirmed", false), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("unconfirmed", false), Some("idle"), false),
            Step::Loaded
        );
    }

    #[test]
    fn a_failed_launch_ends_the_wait_with_its_reason() {
        let mut progress = Progress::default();
        let failed = Some(Launch {
            result: "failed",
            confirmed: false,
            reason: Some("surface_cap"),
        });
        assert_eq!(
            progress.step(failed, None, false),
            Step::Failed("surface_cap".into())
        );
    }

    #[test]
    fn a_template_with_a_request_in_flight_is_not_loaded() {
        let mut progress = Progress::default();
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), true),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), true),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Wait
        );
        assert_eq!(
            progress.step(launch("success", true), Some("idle"), false),
            Step::Loaded
        );
    }
}
