use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::items::{ItemCategory, ItemObservation, ItemSnapshot, ItemUpdate};
use super::project::{LifecycleObservation, ProjectionEffect};
use super::protocol::{Evidence, Field, field};
use super::usage::UsageLedger;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EdgeKind {
    Spawn,
    Message,
    FollowUp,
    Wait,
    Interrupt,
    Resume,
    Close,
    Completion,
    List,
}

impl EdgeKind {
    fn from_tool(tool: &str) -> Option<Self> {
        Some(match tool {
            "spawnAgent" => Self::Spawn,
            "sendInput" | "sendMessage" => Self::Message,
            "followupTask" => Self::FollowUp,
            "wait" => Self::Wait,
            "interruptAgent" => Self::Interrupt,
            "resumeAgent" => Self::Resume,
            "closeAgent" => Self::Close,
            "listAgents" => Self::List,
            _ => return None,
        })
    }

    fn from_activity(kind: &str) -> Option<Self> {
        Some(match kind {
            "started" => Self::Spawn,
            "interacted" => Self::Message,
            "interrupted" => Self::Interrupt,
            "completed" => Self::Completion,
            _ => return None,
        })
    }

    fn next_state(self, current: Option<ChildState>) -> Option<ChildState> {
        match self {
            Self::Spawn | Self::Message | Self::FollowUp | Self::Resume => {
                Some(ChildState::Running)
            }
            Self::Interrupt => Some(ChildState::Interrupted),
            Self::Completion => Some(ChildState::Completed),
            Self::Close => Some(ChildState::Closed),
            Self::Wait | Self::List => current,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeSource {
    CollabToolCall,
    SubAgentActivity,
    ChildTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChildState {
    Running,
    Completed,
    Interrupted,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildEdge {
    pub kind: EdgeKind,
    pub source: EdgeSource,
    pub sender_thread_id: String,
    pub receiver_thread_id: Option<String>,
    pub turn_id: String,
    pub item_id: String,
    pub prompt: Field<String>,
    pub status: Field<String>,
    pub agent_path: Field<String>,
    pub model: Field<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentLink {
    pub parent_thread_id: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    sender: String,
    turn: String,
    item: String,
    receiver: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChildGraph {
    edges: Vec<ChildEdge>,
    index: BTreeMap<EdgeKey, usize>,
    parents: BTreeMap<String, ParentLink>,
    forks: BTreeMap<String, ParentLink>,
    states: BTreeMap<String, ChildState>,
    spawn_order: Vec<String>,
    unrecognised: Vec<Evidence>,
}

impl ChildGraph {
    pub fn apply(&mut self, effect: &ProjectionEffect) {
        match effect {
            ProjectionEffect::Item(item) => self.apply_item(item),
            ProjectionEffect::Lifecycle(lifecycle) => self.apply_lifecycle(lifecycle),
            _ => {}
        }
    }

    pub fn edges(&self) -> &[ChildEdge] {
        &self.edges
    }

    pub fn parent(&self, thread_id: &str) -> Option<&ParentLink> {
        self.parents.get(thread_id)
    }

    pub fn fork_source(&self, thread_id: &str) -> Option<&ParentLink> {
        self.forks.get(thread_id)
    }

    pub fn state(&self, thread_id: &str) -> Option<ChildState> {
        self.states.get(thread_id).copied()
    }

    pub fn unrecognised(&self) -> &[Evidence] {
        &self.unrecognised
    }

    pub fn owning_root<'a>(&'a self, thread_id: &'a str) -> &'a str {
        let mut seen = BTreeSet::from([thread_id]);
        let mut current = thread_id;
        while let Some(link) = self
            .parents
            .get(current)
            .or_else(|| self.forks.get(current))
        {
            if !seen.insert(link.parent_thread_id.as_str()) {
                break;
            }
            current = link.parent_thread_id.as_str();
        }
        current
    }

    pub fn descendants(&self, root: &str) -> Vec<&str> {
        self.parents
            .keys()
            .map(String::as_str)
            .filter(|child| *child != root && self.owning_root(child) == root)
            .collect()
    }

    pub fn running_children(&self) -> Vec<&str> {
        self.spawn_order
            .iter()
            .map(String::as_str)
            .filter(|child| self.state(child) == Some(ChildState::Running))
            .collect()
    }

    fn apply_item(&mut self, item: &ItemObservation) {
        let ItemUpdate::Snapshot(snapshot) = &item.update else {
            return;
        };
        if snapshot.category != ItemCategory::Collaboration {
            return;
        }
        let edges = match snapshot.provider_type.as_str() {
            "subAgentActivity" => activity_edges(item, snapshot),
            "collabAgentToolCall" => tool_call_edges(item, snapshot),
            _ => None,
        };
        let Some(edges) = edges else {
            self.unrecognised.push(item.evidence.clone());
            return;
        };
        for edge in edges {
            self.record(edge);
        }
        if snapshot.provider_type == "collabAgentToolCall" {
            self.apply_agent_states(&snapshot.provider_payload);
        }
    }

    fn record(&mut self, mut edge: ChildEdge) {
        let key = EdgeKey {
            sender: edge.sender_thread_id.clone(),
            turn: edge.turn_id.clone(),
            item: edge.item_id.clone(),
            receiver: edge.receiver_thread_id.clone(),
        };
        if let Some(&at) = self.index.get(&key) {
            if let Some(current) = self.edges.get_mut(at) {
                merge_edge(current, edge);
            }
            return;
        }
        if let Some(child) = edge.receiver_thread_id.clone() {
            if edge.kind == EdgeKind::Message {
                edge.kind = match self.state(&child) {
                    Some(ChildState::Completed) => EdgeKind::FollowUp,
                    Some(ChildState::Interrupted | ChildState::Closed) => EdgeKind::Resume,
                    _ => EdgeKind::Message,
                };
            }
            if edge.kind == EdgeKind::Spawn {
                self.adopt(&child, &edge);
            }
            self.transition(&child, edge.kind);
        }
        self.index.insert(key, self.edges.len());
        self.edges.push(edge);
    }

    fn adopt(&mut self, child: &str, edge: &ChildEdge) {
        if let Some(evidence) = edge.evidence.first() {
            self.parents
                .entry(child.to_owned())
                .or_insert_with(|| ParentLink {
                    parent_thread_id: edge.sender_thread_id.clone(),
                    evidence: evidence.clone(),
                });
        }
        if !self.spawn_order.iter().any(|known| known == child) {
            self.spawn_order.push(child.to_owned());
        }
    }

    fn transition(&mut self, child: &str, kind: EdgeKind) {
        if let Some(state) = kind.next_state(self.state(child)) {
            self.states.insert(child.to_owned(), state);
        }
    }

    fn apply_agent_states(&mut self, payload: &Value) {
        let Some(states) = payload.get("agentsStates").and_then(Value::as_object) else {
            return;
        };
        for (child, state) in states {
            let kind = match state.get("status").and_then(Value::as_str) {
                Some("running" | "pendingInit") => EdgeKind::Resume,
                Some("completed") => EdgeKind::Completion,
                Some("interrupted") => EdgeKind::Interrupt,
                Some("shutdown") => EdgeKind::Close,
                _ => continue,
            };
            self.transition(child, kind);
        }
    }

    fn apply_lifecycle(&mut self, lifecycle: &LifecycleObservation) {
        if let Some(thread) = &lifecycle.thread {
            if let Field::Value(parent) = &thread.parent_thread_id {
                self.link(&lifecycle.thread_id, parent, &lifecycle.evidence, false);
            }
            if let Field::Value(source) = &thread.forked_from_id {
                self.link(&lifecycle.thread_id, source, &lifecycle.evidence, true);
            }
        }
        if !self.parents.contains_key(&lifecycle.thread_id) {
            return;
        }
        match lifecycle.method.as_str() {
            "turn/started" => self.child_turn_started(lifecycle),
            "turn/completed" => {
                let interrupted = lifecycle.status == Field::Value(Value::from("interrupted"));
                let state = if interrupted {
                    ChildState::Interrupted
                } else {
                    ChildState::Completed
                };
                self.states.insert(lifecycle.thread_id.clone(), state);
            }
            _ => {}
        }
    }

    fn link(&mut self, thread: &str, parent: &str, evidence: &Evidence, fork: bool) {
        if thread == parent {
            return;
        }
        let links = if fork {
            &mut self.forks
        } else {
            &mut self.parents
        };
        links.insert(
            thread.to_owned(),
            ParentLink {
                parent_thread_id: parent.to_owned(),
                evidence: evidence.clone(),
            },
        );
        if !fork && !self.spawn_order.iter().any(|known| known == thread) {
            self.spawn_order.push(thread.to_owned());
        }
    }

    fn child_turn_started(&mut self, lifecycle: &LifecycleObservation) {
        let child = &lifecycle.thread_id;
        let settled = matches!(
            self.state(child),
            Some(ChildState::Completed | ChildState::Interrupted | ChildState::Closed)
        );
        if settled {
            let parent = self
                .parents
                .get(child)
                .map(|link| link.parent_thread_id.clone());
            self.record(ChildEdge {
                kind: EdgeKind::Resume,
                source: EdgeSource::ChildTurn,
                sender_thread_id: parent.unwrap_or_default(),
                receiver_thread_id: Some(child.clone()),
                turn_id: lifecycle.turn_id.clone().unwrap_or_default(),
                item_id: String::new(),
                prompt: Field::Missing,
                status: Field::Missing,
                agent_path: Field::Missing,
                model: Field::Missing,
                evidence: vec![lifecycle.evidence.clone()],
            });
        }
        self.states.insert(child.clone(), ChildState::Running);
    }
}

fn base_edge(item: &ItemObservation, kind: EdgeKind, source: EdgeSource) -> ChildEdge {
    ChildEdge {
        kind,
        source,
        sender_thread_id: item.key.thread_id.clone(),
        receiver_thread_id: None,
        turn_id: item.key.turn_id.clone(),
        item_id: item.key.item_id.clone(),
        prompt: Field::Missing,
        status: Field::Missing,
        agent_path: Field::Missing,
        model: Field::Missing,
        evidence: vec![item.evidence.clone()],
    }
}

fn activity_edges(item: &ItemObservation, snapshot: &ItemSnapshot) -> Option<Vec<ChildEdge>> {
    let payload = &snapshot.provider_payload;
    let kind = EdgeKind::from_activity(payload.get("kind")?.as_str()?)?;
    let child = payload.get("agentThreadId")?.as_str()?;
    let mut edge = base_edge(item, kind, EdgeSource::SubAgentActivity);
    edge.receiver_thread_id = Some(child.to_owned());
    edge.agent_path = field(payload, "agentPath");
    edge.model = field(payload, "model");
    edge.status = snapshot.status.clone();
    Some(vec![edge])
}

fn tool_call_edges(item: &ItemObservation, snapshot: &ItemSnapshot) -> Option<Vec<ChildEdge>> {
    let payload = &snapshot.provider_payload;
    let kind = EdgeKind::from_tool(payload.get("tool")?.as_str()?)?;
    let sender = payload
        .get("senderThreadId")
        .and_then(Value::as_str)
        .unwrap_or(item.key.thread_id.as_str());
    let mut edge = base_edge(item, kind, EdgeSource::CollabToolCall);
    edge.sender_thread_id = sender.to_owned();
    edge.prompt = field(payload, "prompt");
    edge.model = field(payload, "model");
    edge.status = field(payload, "status");
    let receivers: Vec<String> = match field::<Vec<String>>(payload, "receiverThreadIds") {
        Field::Value(receivers) => receivers,
        _ => Vec::new(),
    };
    if receivers.is_empty() {
        return Some(vec![edge]);
    }
    Some(
        receivers
            .into_iter()
            .map(|receiver| ChildEdge {
                receiver_thread_id: Some(receiver),
                ..edge.clone()
            })
            .collect(),
    )
}

fn merge_edge(current: &mut ChildEdge, incoming: ChildEdge) {
    for (slot, value) in [
        (&mut current.prompt, incoming.prompt),
        (&mut current.status, incoming.status),
        (&mut current.agent_path, incoming.agent_path),
        (&mut current.model, incoming.model),
    ] {
        if !matches!(value, Field::Missing) {
            *slot = value;
        }
    }
    for evidence in incoming.evidence {
        if !current.evidence.contains(&evidence) {
            current.evidence.push(evidence);
        }
    }
}

pub fn spend_by_root(graph: &ChildGraph, ledger: &UsageLedger) -> BTreeMap<String, i64> {
    let mut roots = BTreeMap::new();
    for spend in ledger.spend() {
        *roots
            .entry(graph.owning_root(&spend.thread_id).to_owned())
            .or_insert(0) += spend.own;
    }
    roots
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyPolicy {
    pub max_active: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyDecision {
    pub active_seats: usize,
    pub active_children: usize,
    pub limit: usize,
    pub admits_another: bool,
    pub interrupt: Vec<String>,
}

impl ConcurrencyPolicy {
    pub fn evaluate(&self, active_seats: usize, graph: &ChildGraph) -> ConcurrencyDecision {
        let running = graph.running_children();
        let room = self.max_active.saturating_sub(active_seats);
        let interrupt = running
            .iter()
            .rev()
            .take(running.len().saturating_sub(room))
            .map(|child| (*child).to_owned())
            .collect();
        ConcurrencyDecision {
            active_seats,
            active_children: running.len(),
            limit: self.max_active,
            admits_another: active_seats + running.len() < self.max_active,
            interrupt,
        }
    }
}
