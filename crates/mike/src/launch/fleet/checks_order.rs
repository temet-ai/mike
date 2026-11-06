use std::collections::{BTreeMap, BTreeSet};

use super::checks::Missing;
use super::definition::FleetDefinition;

pub(super) fn late_checks(def: &FleetDefinition) -> Vec<Missing> {
    let before = predecessors(def);
    let mut found = Vec::new();
    for (id, criterion) in &def.criteria {
        let serving: BTreeSet<&str> = def
            .tasks
            .iter()
            .filter(|(_, task)| task.criteria.contains(id))
            .map(|(task_id, _)| task_id.as_str())
            .collect();
        if serving.is_empty() {
            continue;
        }
        for later in named_tasks(def, &criterion.check) {
            let Some(waits_for) = before.get(later) else {
                continue;
            };
            if !serving.contains(later) && serving.iter().all(|s| waits_for.contains(s)) {
                found.push(Missing::new(
                    format!("criteria.{id}.check"),
                    format!(
                        "the check needs the work of task {later}, which only starts after {} has passed, and {} is all that serves this criterion; serve the criterion from the last task that can satisfy it, so that `after` means a task's own slices passed",
                        join(&serving),
                        join(&serving),
                    ),
                ));
            }
        }
    }
    found
}

fn join(tasks: &BTreeSet<&str>) -> String {
    tasks.iter().copied().collect::<Vec<_>>().join(", ")
}

fn named_tasks<'a>(def: &'a FleetDefinition, text: &str) -> Vec<&'a str> {
    let words: BTreeSet<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    def.tasks
        .keys()
        .map(String::as_str)
        .filter(|id| words.contains(id))
        .collect()
}

fn predecessors(def: &FleetDefinition) -> BTreeMap<&str, BTreeSet<&str>> {
    def.tasks
        .keys()
        .map(|id| (id.as_str(), reach(def, id)))
        .collect()
}

fn reach<'a>(def: &'a FleetDefinition, start: &str) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut queue: Vec<&str> = def
        .tasks
        .get(start)
        .map(|t| t.after.iter().map(String::as_str).collect())
        .unwrap_or_default();
    while let Some(next) = queue.pop() {
        if seen.insert(next)
            && let Some(task) = def.tasks.get(next)
        {
            queue.extend(task.after.iter().map(String::as_str));
        }
    }
    seen
}

pub const LONGEST_CHAIN: usize = 3;

pub(super) fn long_chain(def: &FleetDefinition) -> Vec<Missing> {
    let Some(chain) = longest_chain(def) else {
        return Vec::new();
    };
    if chain.len() <= LONGEST_CHAIN {
        return Vec::new();
    }
    let last = chain.last().copied().unwrap_or_default();
    vec![Missing::new(
        format!("tasks.{last}.after"),
        format!(
            "{} is a chain of {} tasks, each waiting for the one before, so one seat at a time works this chain for most of the run; a chain is at most {LONGEST_CHAIN} tasks: merge links, or make a later link wait only for the task its first step needs",
            chain.join(" → "),
            chain.len()
        ),
    )]
}

fn longest_chain(def: &FleetDefinition) -> Option<Vec<&str>> {
    let mut walk = ChainWalk::default();
    let mut best: Vec<&str> = Vec::new();
    for id in def.tasks.keys() {
        let chain = walk.chain_ending_at(def, id);
        if walk.cyclic {
            return None;
        }
        if chain.len() > best.len() || (chain.len() == best.len() && chain < best) {
            best = chain;
        }
    }
    Some(best)
}

#[derive(Default)]
struct ChainWalk<'a> {
    memo: BTreeMap<&'a str, Vec<&'a str>>,
    on_path: BTreeSet<&'a str>,
    cyclic: bool,
}

impl<'a> ChainWalk<'a> {
    fn chain_ending_at(&mut self, def: &'a FleetDefinition, task: &'a str) -> Vec<&'a str> {
        if let Some(known) = self.memo.get(task) {
            return known.clone();
        }
        if !self.on_path.insert(task) {
            self.cyclic = true;
            return Vec::new();
        }
        let mut longest: Vec<&str> = Vec::new();
        let waits_for = def
            .tasks
            .get(task)
            .map(|t| t.after.as_slice())
            .unwrap_or_default();
        for before in waits_for.iter().filter(|b| def.tasks.contains_key(*b)) {
            let chain = self.chain_ending_at(def, before);
            if chain.len() > longest.len() {
                longest = chain;
            }
        }
        self.on_path.remove(task);
        longest.push(task);
        self.memo.insert(task, longest.clone());
        longest
    }
}
