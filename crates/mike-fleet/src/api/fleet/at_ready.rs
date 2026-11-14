use serde::Serialize;

use super::definition_patch::{STATE_DRAFT, STATE_FAILED, STATE_READY, environment_problems};
use super::definition_view::FleetView;
use super::previous_runs::{self, PreviousRuns};
use crate::base_state::BaseState;
use crate::launch::fleet::capacity::{self, Planned};
use crate::launch::fleet::plan::ready_tasks;
use crate::launch::fleet::{FleetDefinition, Missing, Starts, starts};
use crate::store::persistence::fleet::FleetRow;

const STATES_NOT_YET_STARTED: [&str; 3] = [STATE_DRAFT, STATE_READY, STATE_FAILED];
const AGENTS_DIR: &str = ".claude/agents";

#[derive(Debug, Serialize)]
pub struct AtReady {
    starts: Starts,
    capacity: Option<Planned>,
    machine: Vec<Missing>,
    missing: Vec<ReadyWarning>,
    notes: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_runs: Option<PreviousRuns>,
}

async fn notes(def: &FleetDefinition) -> Vec<&'static str> {
    if crate::launch::fleet::environment::repo_is_linked_worktree(&def.repo).await {
        vec![crate::launch::fleet::environment::LINKED_WORKTREE_NOTE]
    } else {
        Vec::new()
    }
}

#[derive(Debug, Serialize)]
struct ReadyWarning {
    item: String,
    problem: String,
    level: &'static str,
}

fn cold_start_warnings(def: &FleetDefinition, starts: &Starts) -> Vec<ReadyWarning> {
    let ready = ready_tasks(def, &std::collections::BTreeSet::new());
    ready
        .into_iter()
        .filter_map(|id| {
            let task = def.tasks.get(&id)?;
            (!starts.templates.contains(&task.discipline)).then(|| ReadyWarning {
                item: format!("tasks.{id}.discipline"),
                problem: format!(
                    "`{}` has no warm template planned, so its seat will start cold",
                    task.discipline
                ),
                level: "warning",
            })
        })
        .collect()
}

fn agent_roots(def: &FleetDefinition) -> Vec<std::path::PathBuf> {
    vec![
        std::path::Path::new(&def.repo).join(AGENTS_DIR),
        crate::launch::fleet::bundle::home().join(AGENTS_DIR),
    ]
}

fn absent_agent_warnings(def: &FleetDefinition, roots: &[std::path::PathBuf]) -> Vec<ReadyWarning> {
    def.workers
        .iter()
        .filter(|(discipline, worker)| {
            let agent = worker.agent.trim();
            !agent.is_empty()
                && def.tasks.values().any(|task| task.discipline == **discipline)
                && !roots.iter().any(|root| root.join(format!("{agent}.md")).is_file())
        })
        .map(|(discipline, worker)| ReadyWarning {
            item: format!("workers.{discipline}.agent"),
            problem: format!(
                "`{}` has no agent file in the repository's {AGENTS_DIR} or the home one, so its seats run on the fleet-worker skill alone, without the project's own style and checks; write the agent first (the fleet-capability-author agent does this) or name one that exists",
                worker.agent.trim()
            ),
            level: "warning",
        })
        .collect()
}

fn warnings(def: &FleetDefinition, starts: &Starts) -> Vec<ReadyWarning> {
    let mut found = cold_start_warnings(def, starts);
    found.extend(absent_agent_warnings(def, &agent_roots(def)));
    found.extend(
        crate::launch::fleet::checks_paths::compile_unit_warnings(def)
            .into_iter()
            .chain(crate::launch::fleet::operation::unread_warnings(def))
            .chain(crate::launch::fleet::gate::full::warnings(def))
            .chain(crate::launch::fleet::floor::heavy_floor_warnings(def))
            .map(|warning| ReadyWarning {
                item: warning.item,
                problem: warning.problem,
                level: "warning",
            }),
    );
    found
}

pub async fn view(state: &BaseState, row: &FleetRow) -> FleetView {
    let view = FleetView::full(row);
    if !STATES_NOT_YET_STARTED.contains(&row.state.as_str()) {
        return view;
    }
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return view;
    };
    let planned = starts(&def);
    let capacity = capacity::plan(state.persist(), &def, planned.most_at_once).await;
    let previous_runs = match state.persist() {
        Some(persist) => previous_runs::load(persist, &row.project_path, &def).await,
        None => None,
    };
    view.with_at_ready(AtReady {
        previous_runs,
        notes: notes(&def).await,
        missing: warnings(&def, &planned),
        starts: planned,
        capacity: Some(capacity),
        machine: environment_problems(state, row.fleet_uuid, &def).await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::fleet::definition::{TaskDefinition, WorkerDefinition};

    #[test]
    fn previous_runs_ride_in_the_at_ready_answer() -> anyhow::Result<()> {
        let previous = previous_runs::newest(
            "fleet-runs",
            vec![crate::store::persistence::work_threads::HistoryEntry {
                ord: 4,
                kind: "finding".to_string(),
                entry_date: chrono::Utc::now().date_naive(),
                session_uuid: None,
                commit_sha: None,
                summary: "open-issues: a finding".to_string(),
                body: String::new(),
                data: None,
                created_at: chrono::Utc::now(),
            }],
        );
        let value = serde_json::to_value(AtReady {
            starts: starts(&FleetDefinition::default()),
            capacity: None,
            machine: Vec::new(),
            missing: Vec::new(),
            notes: Vec::new(),
            previous_runs: previous,
        })?;
        assert_eq!(
            value["previous_runs"]["findings"][0]["entry"],
            "fleet-runs#4"
        );
        assert!(value["previous_runs"]["ask"].is_string());
        Ok(())
    }

    #[test]
    fn a_planned_cold_start_is_a_warning_in_at_ready_missing() {
        let mut def = FleetDefinition::default();
        def.workers
            .insert("db-dev".to_string(), WorkerDefinition::default());
        def.tasks.insert(
            "P1".to_string(),
            TaskDefinition {
                discipline: "db-dev".to_string(),
                ..TaskDefinition::default()
            },
        );
        let mut planned = starts(&def);
        planned.templates.retain(|name| name != "db-dev");
        let warnings = cold_start_warnings(&def, &planned);
        let Ok(value) = serde_json::to_value(AtReady {
            starts: planned,
            capacity: None,
            machine: Vec::new(),
            missing: warnings,
            notes: Vec::new(),
            previous_runs: None,
        }) else {
            panic!("at_ready must serialize");
        };
        assert_eq!(value["missing"][0]["item"], "tasks.P1.discipline");
        assert_eq!(value["missing"][0]["level"], "warning");
        assert!(
            value["missing"][0]["problem"]
                .as_str()
                .is_some_and(|problem| problem.contains("start cold"))
        );
    }

    #[test]
    fn a_discipline_whose_agent_has_no_file_in_any_root_is_a_warning() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let agents = root.path().join(AGENTS_DIR);
        std::fs::create_dir_all(&agents)?;
        std::fs::write(agents.join("db-dev.md"), "# db-dev\n")?;
        let mut def = FleetDefinition::default();
        for (discipline, agent) in [
            ("db-dev", "db-dev"),
            ("web", "frontend-developer"),
            ("idle", "nobody"),
        ] {
            def.workers.insert(
                discipline.to_string(),
                WorkerDefinition {
                    agent: agent.to_string(),
                    ..WorkerDefinition::default()
                },
            );
        }
        for (id, discipline) in [("P1", "db-dev"), ("P2", "web")] {
            def.tasks.insert(
                id.to_string(),
                TaskDefinition {
                    discipline: discipline.to_string(),
                    ..TaskDefinition::default()
                },
            );
        }
        let found = absent_agent_warnings(&def, &[agents]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].item, "workers.web.agent");
        assert_eq!(found[0].level, "warning");
        assert!(
            found[0]
                .problem
                .contains("`frontend-developer` has no agent file")
        );
        Ok(())
    }
}
