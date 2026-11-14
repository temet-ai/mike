use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use super::stages::{Kind, StageDef};

pub(crate) struct StageInput {
    pub(crate) def: &'static StageDef,
    pub(crate) toggle_key: Option<&'static str>,
    pub(crate) enabled: bool,
    pub(crate) last_ran_at: Option<DateTime<Utc>>,
    pub(crate) running_since: Option<DateTime<Utc>>,
    pub(crate) last_outcome: Option<Value>,
    pub(crate) produced_24h: Option<i64>,
    pub(crate) failing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RunNow {
    pub(crate) method: &'static str,
    pub(crate) path: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Stage {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) kind: Kind,
    pub(crate) toggle_key: Option<&'static str>,
    pub(crate) enabled: bool,
    pub(crate) last_ran_at: Option<DateTime<Utc>>,
    pub(crate) running_since: Option<DateTime<Utc>>,
    pub(crate) last_outcome: Option<Value>,
    pub(crate) produced_24h: Option<i64>,
    pub(crate) fed_by: Option<&'static str>,
    pub(crate) healthy: bool,
    pub(crate) run_now: Option<RunNow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct LearningLoop {
    pub(crate) since: DateTime<Utc>,
    pub(crate) broken_at: Option<&'static str>,
    pub(crate) stages: Vec<Stage>,
}

fn healthy(input: &StageInput, feeder: Option<(&'static str, i64)>) -> bool {
    if !input.enabled {
        return true;
    }
    if input.failing {
        return false;
    }
    let fed = feeder.is_some_and(|(_, produced)| produced > 0);
    !(fed && input.produced_24h == Some(0))
}

pub(crate) fn assemble(since: DateTime<Utc>, inputs: Vec<StageInput>) -> LearningLoop {
    let mut feeder: Option<(&'static str, i64)> = None;
    let mut stages = Vec::with_capacity(inputs.len());
    for input in inputs {
        let ok = healthy(&input, feeder);
        stages.push(Stage {
            id: input.def.id,
            label: input.def.label,
            kind: input.def.kind,
            toggle_key: input.toggle_key,
            enabled: input.enabled,
            last_ran_at: input.last_ran_at,
            running_since: input.running_since,
            last_outcome: input.last_outcome,
            produced_24h: input.produced_24h,
            fed_by: feeder.map(|(id, _)| id),
            healthy: ok,
            run_now: input.def.run_now.map(|path| RunNow {
                method: "POST",
                path,
            }),
        });
        if let Some(produced) = input.produced_24h {
            feeder = Some((input.def.id, produced));
        }
    }
    let broken_at = stages.iter().find(|stage| !stage.healthy).map(|s| s.id);
    LearningLoop {
        since,
        broken_at,
        stages,
    }
}
