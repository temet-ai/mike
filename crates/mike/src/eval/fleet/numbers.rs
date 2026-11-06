use serde::Serialize;
use serde_json::Value;

pub const METRIC_NAMES: [&str; 4] = [
    "time_to_done_minutes",
    "fault_minutes",
    "tokens_per_accepted_task",
    "manual_interventions",
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FleetNumbers {
    pub time_to_done_minutes: Option<f64>,
    pub fault_minutes: Option<f64>,
    pub tokens_per_accepted_task: Option<f64>,
    pub manual_interventions: Option<f64>,
}

fn number_or_total(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.get("total").and_then(Value::as_f64))
}

impl FleetNumbers {
    pub fn from_record(record: &Value) -> Self {
        let read = |key: &str| record.get(key).and_then(number_or_total);
        Self {
            time_to_done_minutes: read(METRIC_NAMES[0]),
            fault_minutes: read(METRIC_NAMES[1]),
            tokens_per_accepted_task: read(METRIC_NAMES[2]),
            manual_interventions: read(METRIC_NAMES[3]),
        }
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "time_to_done_minutes" => self.time_to_done_minutes,
            "fault_minutes" => self.fault_minutes,
            "tokens_per_accepted_task" => self.tokens_per_accepted_task,
            "manual_interventions" => self.manual_interventions,
            _ => None,
        }
    }

    pub fn missing(&self) -> Vec<&'static str> {
        METRIC_NAMES
            .into_iter()
            .filter(|name| self.get(name).is_none())
            .collect()
    }
}
