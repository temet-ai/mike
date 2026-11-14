use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::definition::FleetDefinition;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Execution {
    pub operations: BTreeMap<String, Operation>,
    pub timed_commands: BTreeMap<String, TimedCommand>,
    pub background_commands: Vec<String>,
    pub strip_environment: Vec<String>,
    pub disabled_plugins: Vec<String>,
    pub build_directory_settings: Vec<String>,
    pub package_manifests: Vec<PackageManifest>,
    #[serde(flatten)]
    pub unread: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceClaim {
    Name(String),
    Counted(CountedResource),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountedResource {
    pub name: String,
    pub capacity: u32,
}

pub const MAX_CAPACITY: u32 = 64;

impl ResourceClaim {
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Counted(counted) => &counted.name,
        }
    }

    pub fn capacity(&self) -> u32 {
        match self {
            Self::Name(_) => 1,
            Self::Counted(counted) => counted.capacity,
        }
    }

    fn describe(&self) -> String {
        match self.capacity() {
            1 => self.name().to_string(),
            capacity => format!("{} ({capacity} at once)", self.name()),
        }
    }
}

impl From<String> for ResourceClaim {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

impl From<&str> for ResourceClaim {
    fn from(name: &str) -> Self {
        Self::Name(name.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operation {
    pub command: String,
    pub purpose: String,
    pub resources: Vec<ResourceClaim>,
    pub environment: BTreeMap<String, String>,
    pub inputs: Vec<String>,
    pub cache_key: Option<String>,
    pub timeout_seconds: Option<u64>,
}

impl Execution {
    pub fn named(&self, command: &str) -> Option<(&str, &Operation)> {
        self.operations
            .iter()
            .find(|(_, op)| op.command.trim() == command.trim())
            .map(|(name, op)| (name.as_str(), op))
    }

    pub fn operation(&self, command: &str) -> Option<&Operation> {
        self.named(command).map(|(_, op)| op)
    }
}

pub fn declared_timeout(def: &FleetDefinition) -> Option<Duration> {
    def.execution
        .operation(&def.check.command)
        .and_then(|op| op.timeout_seconds)
        .map(Duration::from_secs)
}

pub fn timeout(def: &FleetDefinition) -> Duration {
    def.execution
        .operation(&def.check.command)
        .and_then(|op| op.timeout_seconds)
        .map_or_else(
            || {
                super::managed_queue::patience_from(super::managed_queue::measured(
                    &def.check.command,
                ))
            },
            Duration::from_secs,
        )
}

pub fn reusable(def: &FleetDefinition) -> bool {
    reusable_command(def, &def.check.command)
}

pub fn reusable_command(def: &FleetDefinition, command: &str) -> bool {
    def.execution.operation(command).is_some_and(|op| {
        op.cache_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
    })
}

pub fn description(def: &FleetDefinition, command: &str) -> String {
    match def.execution.named(command) {
        Some((name, op)) => format!("declared operation {name}: {} — {command}", op.purpose),
        None => format!("command completion only; no declared proof purpose: {command}"),
    }
}

pub fn adoption(def: &FleetDefinition) -> serde_json::Value {
    serde_json::json!({
        "declared_operations": def.execution.operations.len(),
        "check_declared": def.execution.operation(&def.check.command).is_some(),
        "gate_declared": def.execution.operation(&def.gate.command).is_some(),
        "timed_commands": def.execution.timed_commands.len(),
        "next_action": "Declare project-specific proof commands and route exact commands through fleet_check; wrapped and undeclared commands are outside bypass coverage."
    })
}

pub fn context(def: &FleetDefinition) -> String {
    let mut lines = vec![
        "Run a declared operation with fleet_check(command: exact command) and carry on until its result mail; run in your own shell, no other seat can reuse its result. An undeclared command proves only that it finished."
            .to_string(),
    ];
    for (name, op) in &def.execution.operations {
        let line = format!(
            "- {name}: `{}` — {}. Exclusive host resources: {}. Completed-result reuse: {}.",
            op.command,
            op.purpose,
            if op.resources.is_empty() {
                "none".into()
            } else {
                op.resources
                    .iter()
                    .map(ResourceClaim::describe)
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            if op.cache_key.is_some() {
                "declared clean-source inputs and environment only"
            } else {
                "disabled"
            },
        );
        if lines.iter().map(String::len).sum::<usize>() + line.len() > 4000 {
            lines.push("Further operation declarations are available in the fleet definition; retrieve the operation needed for this task.".into());
            break;
        }
        lines.push(line);
    }
    if !def.execution.timed_commands.is_empty() {
        let programs: Vec<&str> = def
            .execution
            .timed_commands
            .keys()
            .map(String::as_str)
            .collect();
        lines.push(format!(
            "This project times your own runs of {} and queues them on the resources declared for them.",
            programs.join(", ")
        ));
    }
    if !def.execution.background_commands.is_empty() {
        lines.push(format!(
            "This project runs these in the background when you start them in the foreground: {}.",
            def.execution.background_commands.join(", ")
        ));
    }
    lines.join("\n")
}

#[path = "operation_settings.rs"]
pub mod settings;
pub use settings::{PackageManifest, TimedCommand};

#[path = "operation_validation.rs"]
pub mod validation;
pub use validation::unread_warnings;
pub use validation::validate;

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
