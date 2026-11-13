pub mod splice;
#[cfg(test)]
pub mod tests;

use std::collections::VecDeque;
use std::sync::Mutex;

use bytes::Bytes;
use dashmap::DashMap;

use crate::extract::signals::RequestSignals;
use crate::pipeline::StageBody;
use crate::wire::manifest::AppliedStage;
use splice::InjectedBlocks;

pub const STAGE: &str = "injected_block_replay";
const DEFAULT_LANE_CAPACITY: usize = 256;
const MAX_RECORDS_PER_LANE: usize = 32;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub replayed_indices: Vec<usize>,
    pub dropped: usize,
}

#[derive(Debug)]
pub struct InjectedBlockRegistry {
    lanes: DashMap<String, Vec<InjectedBlocks>>,
    order: Mutex<VecDeque<String>>,
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
            capacity: capacity.max(1),
        }
    }

    pub fn replay(&self, lane: &str, body: &[u8]) -> Option<(Bytes, ReplayReport)> {
        let mut entry = self.lanes.get_mut(lane)?;
        let mut current = body.to_vec();
        let mut report = ReplayReport::default();
        entry.retain(|record| match splice::splice(&current, record) {
            Some(next) => {
                current = next;
                report.replayed_indices.push(record.index);
                true
            }
            None => {
                report.dropped += 1;
                false
            }
        });
        let emptied = entry.is_empty();
        drop(entry);
        if emptied {
            self.lanes.remove(lane);
        }
        Some((Bytes::from(current), report))
    }

    pub fn record(&self, lane: &str, original: &[u8], injected: &[u8]) -> bool {
        let Some(captured) = splice::capture(original, injected) else {
            return false;
        };
        let is_new = !self.lanes.contains_key(lane);
        {
            let mut records = self.lanes.entry(lane.to_string()).or_default();
            records.retain(|r| r.index != captured.index);
            records.push(captured);
            records.sort_by_key(|r| r.index);
            while records.len() > MAX_RECORDS_PER_LANE {
                records.remove(0);
            }
        }
        if is_new {
            self.track_lane(lane);
        }
        true
    }

    fn track_lane(&self, lane: &str) {
        let mut order = match self.order.lock() {
            Ok(o) => o,
            Err(poisoned) => poisoned.into_inner(),
        };
        order.retain(|l| l != lane);
        order.push_back(lane.to_string());
        while order.len() > self.capacity {
            if let Some(oldest) = order.pop_front() {
                self.lanes.remove(&oldest);
            }
        }
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn records_in(&self, lane: &str) -> usize {
        self.lanes.get(lane).map_or(0, |r| r.len())
    }
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
    let parameters = serde_json::json!({
        "message_indices": report.replayed_indices,
        "dropped": report.dropped,
    });
    if report.replayed_indices.is_empty() {
        body.decline(STAGE, "injected_block_message_changed", parameters);
        return;
    }
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
