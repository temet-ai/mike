use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FleetDefinition {
    pub goal: String,
    pub ask: String,
    pub repo: String,
    pub branch: String,
    pub rules: Vec<String>,
    pub shared_files: Vec<String>,
    pub shared_file_producers: BTreeMap<String, String>,
    pub criteria: BTreeMap<String, Criterion>,
    pub coordinator: SeatDefinition,
    pub observer: SeatDefinition,
    pub retrospective: super::retrospective::Retrospective,
    pub retrospective_job: Option<super::retrospective::Job>,
    pub acceptance: SeatDefinition,
    pub workers: BTreeMap<String, WorkerDefinition>,
    pub tasks: BTreeMap<String, TaskDefinition>,
    pub check: CheckDefinition,
    pub commit_check: CommitChecks,
    pub gate: GateDefinition,
    pub baseline: Option<super::start_baseline::Baseline>,
    pub reference: ReferenceDefinition,
    pub template_limit_tokens: Option<i64>,
    pub seat_settings: Option<Value>,
    pub wrappers: Vec<String>,
    pub execution: super::operation::Execution,
    pub briefs: BriefsDefinition,
    pub staffing: StaffingDefinition,
    pub proposals: BTreeMap<String, ProposalDefinition>,
    pub goals: GoalsDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StaffingDefinition {
    pub adopt_unowned: bool,
    pub warm_siblings: bool,
}

impl Default for StaffingDefinition {
    fn default() -> Self {
        Self {
            adopt_unowned: true,
            warm_siblings: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoalsDefinition {
    pub workers: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProposalDefinition {
    pub name: String,
    pub why: String,
    pub files: Vec<String>,
    pub new_files: Vec<String>,
    pub criteria: Vec<String>,
    pub after: Vec<String>,
    pub discipline: String,
    pub by: String,
    pub state: String,
    pub task: Option<String>,
    pub owners: Vec<String>,
    pub causes: Vec<String>,
    pub problems: Vec<String>,
    pub at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BriefsDefinition {
    pub as_notes: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GateDefinition {
    pub command: String,
    pub timeout_pattern: String,
    pub failure_pattern: String,
    pub rerun_command: String,
}

impl GateDefinition {
    pub fn declared(&self) -> bool {
        !self.command.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CommitChecks(pub Vec<CommitCheckDefinition>);

impl<'de> Deserialize<'de> for CommitChecks {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Shape {
            Many(Vec<CommitCheckDefinition>),
            One(CommitCheckDefinition),
        }
        Ok(match Shape::deserialize(deserializer)? {
            Shape::Many(many) => Self(many),
            Shape::One(one) => Self(vec![one]),
        })
    }
}

impl CommitChecks {
    pub fn declared(&self) -> Vec<&CommitCheckDefinition> {
        self.0.iter().filter(|c| c.declared()).collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitCheckDefinition {
    pub command: String,
    pub files: Vec<String>,
}

impl CommitCheckDefinition {
    pub fn declared(&self) -> bool {
        !self.command.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CheckDefinition {
    pub command: String,
    pub paths: Vec<String>,
    pub test_command: String,
}

impl CheckDefinition {
    pub fn declared(&self) -> bool {
        !self.command.trim().is_empty()
    }

    pub fn covered(&self) -> Vec<String> {
        self.paths
            .iter()
            .map(|p| super::checks::normal(p))
            .filter(|p| !p.is_empty() && p != ".")
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceDefinition {
    pub enabled: bool,
    pub discipline: Option<String>,
    pub session: Option<String>,
}

impl Default for ReferenceDefinition {
    fn default() -> Self {
        Self {
            enabled: true,
            discipline: None,
            session: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Criterion {
    pub text: String,
    pub check: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeatDefinition {
    pub runtime: crate::runtime::RuntimeKind,
    pub model: String,
    pub effort: Option<String>,
    pub agent: String,
    pub brief: String,
    pub contract: Option<String>,
    pub session: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerDefinition {
    pub runtime: crate::runtime::RuntimeKind,
    pub model: String,
    pub effort: Option<String>,
    pub agent: String,
    pub contract: Option<String>,
}

pub const WORKER_SKILL_CONTRACT: &str = ".claude/fleet/worker-contract.md";

impl WorkerDefinition {
    pub fn contract_file(&self) -> String {
        self.contract
            .as_deref()
            .map(str::trim)
            .filter(|file| !file.is_empty())
            .unwrap_or(WORKER_SKILL_CONTRACT)
            .to_string()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskDefinition {
    pub name: String,
    pub discipline: String,
    pub criteria: Vec<String>,
    pub files: Vec<String>,
    pub new_files: Vec<String>,
    pub required_paths: BTreeMap<String, String>,
    pub brief: String,
    pub goal: String,
    pub after: Vec<String>,
    pub stub_by: Option<u32>,
    pub wait_for_commits: Vec<CommitWait>,
    pub seat: Option<String>,
    pub session: Option<String>,
    pub state: Option<String>,
    pub held: Option<SiblingWait>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_when: Option<DoneWhen>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DoneWhen {
    pub checks: Vec<DoneCheck>,
    pub tests_added_under: Vec<String>,
    pub files_changed: Vec<String>,
    pub exercise: Option<DoneExercise>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DoneCheck {
    pub operation: String,
    pub expected_exit: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DoneExercise {
    pub command: String,
    pub expect_output: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitWait {
    pub task: String,
    pub paths: Vec<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiblingWait {
    pub since: DateTime<Utc>,
    pub waiting_for: String,
}

impl FleetDefinition {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        serde_json::from_value(value.clone())
            .map_err(|e| format!("the definition does not have the expected shape: {e}"))
    }
}

pub fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(changes) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    let Some(fields) = target.as_object_mut() else {
        return;
    };
    for (key, change) in changes {
        if change.is_null() {
            fields.remove(key);
        } else {
            merge(fields.entry(key.clone()).or_insert(Value::Null), change);
        }
    }
}

pub fn natural_order(id: &str) -> (String, u64, String) {
    let digits_at = id.find(|c: char| c.is_ascii_digit()).unwrap_or(id.len());
    let (prefix, rest) = id.split_at(digits_at);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse().unwrap_or(0);
    (prefix.to_string(), number, rest[digits.len()..].to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRun {
    pub seat: Option<String>,
    pub session: Option<uuid::Uuid>,
    pub state: String,
}

pub fn task_run(status: &super::status::FleetStatus, task: &str) -> Option<TaskRun> {
    let found = status.tasks.iter().find(|t| t.id == task)?;
    let session = found.seat.as_deref().and_then(|name| {
        status
            .seats
            .iter()
            .find(|seat| seat.seat == name)
            .and_then(|seat| seat.session)
    });
    Some(TaskRun {
        seat: found.seat.clone(),
        session,
        state: found.state.clone(),
    })
}

pub async fn task_session(
    persist: &crate::store::persistence::PersistHandle,
    run: &str,
    task: &str,
) -> Option<uuid::Uuid> {
    let events = persist.fleet_events_for_task(run, task).await.ok()?;
    super::fold::fold(&events, DateTime::<Utc>::MAX_UTC).seat_of(task)
}

pub fn with_task_seats(
    mut def: FleetDefinition,
    events: &[super::events::FleetEvent],
) -> FleetDefinition {
    let folded = super::fold::fold(events, DateTime::<Utc>::MAX_UTC);
    for (id, task) in &mut def.tasks {
        if let Some(session) = folded.seat_of(id).or_else(|| folded.last_seat(id)) {
            task.session = Some(session.to_string());
        }
    }
    def
}

pub fn recorded_task_of(
    events: &[super::events::FleetEvent],
    session: uuid::Uuid,
) -> Option<String> {
    super::fold::fold(events, DateTime::<Utc>::MAX_UTC)
        .task_of(session)
        .map(str::to_string)
}

pub async fn task_sessions(
    persist: &crate::store::persistence::PersistHandle,
    run: &str,
    def: &FleetDefinition,
) -> BTreeMap<String, uuid::Uuid> {
    let Ok(events) = persist.fleet_events_since(run, 0).await else {
        return BTreeMap::new();
    };
    let folded = super::fold::fold(&events, DateTime::<Utc>::MAX_UTC);
    def.tasks
        .keys()
        .filter_map(|task| Some((task.clone(), folded.seat_of(task)?)))
        .collect()
}
