use serde_json::{Value, json};
use uuid::Uuid;

use super::bundle::FleetPaths;
use super::definition::{FleetDefinition, merge};
use super::plan::{ControlSeats, FleetFacts};

pub fn complete() -> Value {
    json!({
        "goal": "The improvement pages show what the judge did",
        "ask": "make the judge's work visible",
        "repo": "/srv/project",
        "branch": "main",
        "rules": ["no seat works on its own branch"],
        "shared_files": ["crates/mike/src/store/persistence/mod.rs"],
        "criteria": {
            "C1": {"text": "proposals are stored", "check": "the integration test passes"},
            "C2": {"text": "the page lists proposals", "check": "the story test passes"}
        },
        "coordinator": {"model": "opus", "agent": "fleet-coordinator", "brief": "Keep every seat busy."},
        "observer": {"model": "sonnet", "agent": "fleet-observer", "brief": "Watch the run.",
                     "contract": ".claude/fleet/observer-contract.md"},
        "acceptance": {"model": "opus", "agent": "fleet-acceptance", "brief": "Judge each slice."},
        "workers": {
            "db-dev": {"model": "sonnet", "agent": "db-dev",
                       "contract": ".claude/fleet/worker-contract.md"},
            "frontend-developer": {"model": "sonnet", "agent": "frontend-developer",
                                   "contract": ".claude/fleet/worker-contract.md"}
        },
        "tasks": {
            "P1": {"name": "schema", "discipline": "db-dev", "criteria": ["C1"],
                   "files": ["crates/mike/src/store/persistence/improvement/",
                             "crates/mike/src/store/persistence/mod.rs"],
                   "brief": "Add the proposals table."},
            "P2": {"name": "page", "discipline": "frontend-developer", "criteria": ["C2"],
                   "files": ["frontend/src/routes/improvement/**"], "brief": "Build the page."},
            "P10": {"name": "docs", "discipline": "frontend-developer", "criteria": ["C2"],
                    "files": ["docs/IMPROVEMENT_LOOP.md"], "brief": "Write it up.", "after": ["P2"]}
        }
    })
}

pub fn parsed(value: &Value) -> FleetDefinition {
    FleetDefinition::from_value(value).unwrap_or_default()
}

pub fn changed(patch: Value) -> FleetDefinition {
    let mut value = complete();
    merge(&mut value, &patch);
    parsed(&value)
}

pub fn seats() -> ControlSeats {
    ControlSeats {
        coordinator: Uuid::from_u128(1),
        acceptance: Uuid::from_u128(2),
        observer: None,
    }
}

pub fn facts() -> FleetFacts {
    FleetFacts {
        fleet: "0b6c1f0e-1111-4222-8333-444455556666".into(),
        slug: "judge-at-work".into(),
        run_id: "fleet-judge-at-work-0b6c1f0e".into(),
        seats: seats(),
        paths: FleetPaths {
            rules: "/srv/project/.claude/fleet/rules.md".into(),
            checks: "/srv/project/.claude/fleet/outcome-checks.sh".into(),
            project: Some("/srv/project/.claude/fleet/project.md".into()),
        },
    }
}
