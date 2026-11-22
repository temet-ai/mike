pub mod splice;
pub mod store;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use bytes::Bytes;
use dashmap::DashMap;

use crate::extract::signals::RequestSignals;
use crate::pipeline::StageBody;
use crate::wire::manifest::AppliedStage;
use splice::InjectedBlocks;

pub const STAGE: &str = "injected_block_replay";
const DEFAULT_LANE_CAPACITY: usize = 256;
const MAX_RECORDS_PER_LANE: usize = 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub replayed_indices: Vec<usize>,
    pub held: usize,
}

#[derive(Debug, Default)]
struct LaneRecords {
    by_identity: HashMap<u64, InjectedBlocks>,
    order: VecDeque<u64>,
}

impl LaneRecords {
    fn remove(&mut self, identity: u64) {
        if self.by_identity.remove(&identity).is_some() {
            self.order.retain(|i| *i != identity);
        }
    }

    fn insert(&mut self, record: InjectedBlocks) {
        let identity = record.identity;
        if self.by_identity.insert(identity, record).is_none() {
            self.order.push_back(identity);
        }
        while self.order.len() > MAX_RECORDS_PER_LANE {
            if let Some(oldest) = self.order.pop_front() {
                self.by_identity.remove(&oldest);
            }
        }
    }
}

#[derive(Debug)]
pub struct InjectedBlockRegistry {
    lanes: DashMap<String, LaneRecords>,
    order: Mutex<VecDeque<String>>,
    in_use: Mutex<HashSet<String>>,
    capacity: usize,
}

impl Default for InjectedBlockRegistry {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_LANE_CAPACITY)
    }
}

impl InjectedBlockRegistry {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            lanes: DashMap::new(),
            order: Mutex::new(VecDeque::new()),
            in_use: Mutex::new(HashSet::new()),
            capacity: capacity.max(1),
        }
    }

    pub fn replay(&self, lane: &str, body: &[u8]) -> Option<(Bytes, ReplayReport)> {
        let entry = self.lanes.get(lane)?;
        let spliced = splice::splice_all(body, &entry.by_identity)?;
        let report = ReplayReport {
            held: entry.by_identity.len(),
            replayed_indices: spliced.message_indices,
        };
        drop(entry);
        if spliced.continuation || !report.replayed_indices.is_empty() {
            lock(&self.in_use).insert(lane.to_owned());
        }
        Some((Bytes::from(spliced.body), report))
    }

    pub fn take_lanes_in_use(&self) -> Vec<String> {
        lock(&self.in_use).drain().collect()
    }

    pub fn mark_in_use(&self, lanes: Vec<String>) {
        lock(&self.in_use).extend(lanes);
    }

    pub fn forget(&self, expired: &[(String, u64)]) {
        let mut emptied = Vec::new();
        for (lane, identity) in expired {
            let Some(mut records) = self.lanes.get_mut(lane) else {
                continue;
            };
            records.remove(*identity);
            if records.by_identity.is_empty() {
                emptied.push(lane.clone());
            }
        }
        for lane in emptied {
            self.lanes.remove_if(&lane, |_, r| r.by_identity.is_empty());
            if !self.lanes.contains_key(&lane) {
                lock(&self.order).retain(|l| l != &lane);
            }
        }
    }

    pub fn record(&self, lane: &str, original: &[u8], injected: &[u8]) -> bool {
        self.capture(lane, original, injected).is_some()
    }

    pub fn capture(&self, lane: &str, original: &[u8], injected: &[u8]) -> Option<InjectedBlocks> {
        let captured = splice::capture(original, injected)?;
        self.restore(lane, captured.clone());
        Some(captured)
    }

    pub fn restore(&self, lane: &str, record: InjectedBlocks) {
        let is_new = !self.lanes.contains_key(lane);
        self.lanes
            .entry(lane.to_string())
            .or_default()
            .insert(record);
        if is_new {
            self.track_lane(lane);
        }
    }

    fn track_lane(&self, lane: &str) {
        let mut order = lock(&self.order);
        order.retain(|l| l != lane);
        order.push_back(lane.to_string());
        while order.len() > self.capacity {
            if let Some(oldest) = order.pop_front() {
                self.lanes.remove(&oldest);
            }
        }
    }

    #[cfg(test)]
    pub fn records_in(&self, lane: &str) -> usize {
        self.lanes.get(lane).map_or(0, |r| r.by_identity.len())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn lane_key(signals: &RequestSignals) -> String {
    let lane = match (&signals.claude_code_agent_id, &signals.turn_origin) {
        (Some(agent), _) => format!("agent:{agent}"),
        (
            None,
            crate::extract::request::TurnOrigin::Primary
            | crate::extract::request::TurnOrigin::PrimaryFirst,
        ) => "primary".to_string(),
        (None, origin) => origin.to_string(),
    };
    format!("{}:{lane}", signals.session_id)
}

pub fn replay_into(registry: &InjectedBlockRegistry, lane: &str, body: &mut StageBody) {
    let Some((new_body, report)) = registry.replay(lane, body.body()) else {
        return;
    };
    if report.replayed_indices.is_empty() {
        return;
    }
    let parameters = serde_json::json!({
        "message_indices": report.replayed_indices,
        "held": report.held,
    });
    body.rewrite(
        new_body.clone(),
        AppliedStage {
            stage: STAGE,
            policy_id: STAGE,
            policy_version: 1,
            resolved_parameters: parameters,
            new_bytes: new_body.as_ref(),
            targets: Vec::new(),
            reason_code: "injected_block_replayed",
            detail: Some(format!(
                "replayed into messages {:?}",
                report.replayed_indices
            )),
        },
    );
}
