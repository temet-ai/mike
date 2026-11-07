use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Execution, ResourceClaim};
use crate::launch::fleet::work_of::{through_wrapper, work_of};

pub type SharedResource<'a> = (usize, Vec<(&'a str, &'a TimedCommand)>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TimedCommand {
    pub subcommands: Vec<String>,
    pub resources: Vec<ResourceClaim>,
    pub through: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PackageManifest {
    pub file: String,
    pub contains: String,
    pub label: String,
}

impl TimedCommand {
    pub fn measures<'a>(&self, words: impl IntoIterator<Item = &'a str>) -> bool {
        let mut words = words.into_iter().take_while(|word| *word != "--");
        self.subcommands.is_empty()
            || words.any(|word| self.subcommands.iter().any(|sub| sub == word))
    }

    pub fn covers(&self, program: &str, command: &str, wrappers: &[String]) -> bool {
        let through: Vec<String> = wrappers.iter().chain(&self.through).cloned().collect();
        let wrapped = through_wrapper(command, &through);
        work_of(command).iter().any(|work| {
            let mut words = work.split_whitespace();
            let direct = words.next() == Some(program);
            (direct || wrapped) && self.measures(words)
        })
    }
}

impl Execution {
    pub fn shared_resources(&self) -> BTreeMap<&str, SharedResource<'_>> {
        let mut shared: BTreeMap<&str, SharedResource<'_>> = BTreeMap::new();
        for (program, timed) in &self.timed_commands {
            for claim in &timed.resources {
                let entry = shared
                    .entry(claim.name().trim())
                    .or_insert_with(|| (claim.capacity().max(1) as usize, Vec::new()));
                entry.1.push((program, timed));
            }
        }
        shared
    }

    pub fn environment_policy(&self) -> serde_json::Value {
        if self.strip_environment.is_empty() {
            serde_json::Value::Bool(false)
        } else {
            serde_json::json!(self.strip_environment)
        }
    }
}
