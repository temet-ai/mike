use serde::Serialize;

use super::definition_patch::{STATE_DRAFT, STATE_FAILED, STATE_READY, environment_problems};
use super::definition_view::FleetView;
use crate::config::AppState;
use crate::launch::fleet::plan::ready_tasks;
use crate::launch::fleet::{FleetDefinition, Missing, Starts, starts};
use crate::store::persistence::fleet::FleetRow;

const STATES_NOT_YET_STARTED: [&str; 3] = [STATE_DRAFT, STATE_READY, STATE_FAILED];
const AGENTS_DIR: &str = ".claude/agents";

#[derive(Debug, Serialize)]
pub(crate) struct AtReady {
    starts: Starts,
    machine: Vec<Missing>,
    missing: Vec<ReadyWarning>,
    notes: Vec<&'static str>,
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
            .map(|warning| ReadyWarning {
                item: warning.item,
                problem: warning.problem,
                level: "warning",
            }),
    );
    found
}

pub(super) async fn view(state: &AppState, row: &FleetRow) -> FleetView {
    let view = FleetView::full(row);
    if !STATES_NOT_YET_STARTED.contains(&row.state.as_str()) {
        return view;
    }
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return view;
    };
    let planned = starts(&def);
    view.with_at_ready(AtReady {
        notes: notes(&def).await,
        missing: warnings(&def, &planned),
        starts: planned,
        machine: environment_problems(state, row.fleet_uuid, &def).await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::fleet::definition::{TaskDefinition, WorkerDefinition};

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
            machine: Vec::new(),
            missing: warnings,
            notes: Vec::new(),
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
