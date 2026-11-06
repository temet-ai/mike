pub mod observation;
pub mod read_back;
pub mod tool_effect;
pub mod wait_backing;

#[cfg(test)]
#[path = "tests/observation_tests.rs"]
mod observation_tests;
#[cfg(test)]
#[path = "tests/tool_effect_tests.rs"]
mod tool_effect_tests;
#[cfg(test)]
#[path = "tests/wait_backing_tests.rs"]
mod wait_backing_tests;

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::steering_facts::{
    BackingRow, CallRow, FactKey, StaleFilter, SteeringFactsWrite,
};
use crate::store::persistence::{PersistError, PersistHandle};
use observation::{CallFact, Observation, observe};
use wait_backing::{BackingRef, REGISTRY_PARSES_SCHEDULED_LAUNCHES, WaitBacking, decide};

pub const STEERING_FACTS_VERSION: i16 = 1;
const BATCH: i64 = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassFact {
    pub class: &'static str,
    pub rule: &'static str,
    pub launch_id: Option<i64>,
    pub run_id: Option<String>,
    pub seat: Option<String>,
    pub template_id: Option<i64>,
    pub fork: bool,
    pub role: Option<String>,
    pub later_genuine_turn_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SteeringFacts {
    pub wait_backing: WaitBacking,
    pub wait_backing_refs: Vec<BackingRef>,
    pub observation: Observation,
    pub session_class: Option<ClassFact>,
    pub as_of: DateTime<Utc>,
    pub version: i16,
}

fn to_ref(row: &BackingRow) -> BackingRef {
    BackingRef {
        source: row.source.clone(),
        id: row.id,
        kind: row.kind.clone(),
        state: row.state.clone(),
        launched_at: row.launched_at,
        due_at: row.due_at,
        owner_agent_id: row.owner_agent_id.clone(),
        peer_session_uuid: row.peer_session_uuid.clone(),
        message_uuid: row.message_uuid.clone(),
        directive_id: row.directive_id,
    }
}

fn to_call(row: &CallRow) -> CallFact {
    CallFact {
        use_id: row.use_id,
        at: row.at,
        tool_name: row.tool_name.clone(),
        input: row.input.clone(),
        has_result: row.has_result,
    }
}

pub struct ComputedFacts {
    pub key: FactKey,
    pub wait_backing: WaitBacking,
    pub refs: Vec<BackingRef>,
    pub observation: Observation,
}

pub async fn compute(
    persist: &PersistHandle,
    keys: &[FactKey],
) -> Result<Vec<ComputedFacts>, PersistError> {
    let backing = persist
        .steering_backing_rows(keys, !REGISTRY_PARSES_SCHEDULED_LAUNCHES)
        .await?;
    let calls = persist.steering_call_rows(keys).await?;
    let mut refs_by_turn: HashMap<i64, Vec<BackingRef>> = HashMap::new();
    for row in &backing {
        refs_by_turn
            .entry(row.turn_id)
            .or_default()
            .push(to_ref(row));
    }
    let mut calls_by_turn: HashMap<i64, Vec<CallFact>> = HashMap::new();
    for row in &calls {
        calls_by_turn
            .entry(row.turn_id)
            .or_default()
            .push(to_call(row));
    }
    Ok(keys
        .iter()
        .map(|key| {
            let (wait_backing, refs) =
                decide(refs_by_turn.remove(&key.turn_id).unwrap_or_default());
            let observation = observe(
                calls_by_turn
                    .get(&key.turn_id)
                    .map_or(&[][..], Vec::as_slice),
            );
            ComputedFacts {
                key: *key,
                wait_backing,
                refs,
                observation,
            }
        })
        .collect())
}

fn class_fact(result: crate::dossier::build::session_class::ClassResult) -> ClassFact {
    let e = result.evidence;
    ClassFact {
        class: result.class.code(),
        rule: result.rule,
        launch_id: e.launch_id,
        run_id: e.run_id,
        seat: e.seat,
        template_id: e.template_id,
        fork: e.fork,
        role: e.role,
        later_genuine_turn_id: e.later_genuine_turn_id,
    }
}

impl SteeringFacts {
    pub async fn as_of(
        persist: &PersistHandle,
        session_db_id: i64,
        turn_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Self, PersistError> {
        let key = FactKey {
            session_id: session_db_id,
            turn_id,
            as_of,
        };
        let mut computed = compute(persist, &[key]).await?;
        let facts = computed.pop();
        let class =
            crate::dossier::build::classify_session_as_of(persist, session_db_id, as_of).await?;
        Ok(Self {
            wait_backing: facts.as_ref().map_or(WaitBacking::None, |f| f.wait_backing),
            wait_backing_refs: facts.as_ref().map(|f| f.refs.clone()).unwrap_or_default(),
            observation: facts.map(|f| f.observation).unwrap_or_default(),
            session_class: class.map(class_fact),
            as_of,
            version: STEERING_FACTS_VERSION,
        })
    }
}

fn write_row(f: ComputedFacts) -> Result<SteeringFactsWrite, PersistError> {
    Ok(SteeringFactsWrite {
        operator_turn_id: f.key.turn_id,
        wait_backing: f.wait_backing.code().to_owned(),
        wait_backing_refs: serde_json::to_value(&f.refs)?,
        observed_after_change: f.observation.observed_after_change,
        last_change_use_id: f.observation.last_change_use_id,
        last_change_at: f.observation.last_change_at,
        last_observation_use_id: f.observation.last_observation_use_id,
        last_observation_at: f.observation.last_observation_at,
        observation_kind: f.observation.observation_kind.map(str::to_owned),
        steering_facts_version: STEERING_FACTS_VERSION,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FillReport {
    pub turns: u64,
    pub written: u64,
    pub by_wait_backing: BTreeMap<String, u64>,
    pub by_observation_kind: BTreeMap<String, u64>,
}

impl FillReport {
    fn count(&mut self, rows: &[SteeringFactsWrite]) {
        for row in rows {
            *self
                .by_wait_backing
                .entry(row.wait_backing.clone())
                .or_default() += 1;
            let kind = row
                .observation_kind
                .clone()
                .unwrap_or_else(|| "none".to_owned());
            *self.by_observation_kind.entry(kind).or_default() += 1;
        }
    }
}

pub async fn fill(
    persist: &PersistHandle,
    filter: StaleFilter,
    execute: bool,
) -> Result<FillReport, PersistError> {
    let mut report = FillReport::default();
    let mut after_turn = 0_i64;
    loop {
        let page = StaleFilter {
            limit: BATCH,
            after_turn,
            ..filter.clone()
        };
        let batch = persist
            .stale_steering_fact_turns(&page, STEERING_FACTS_VERSION)
            .await?;
        let Some(last) = batch.last() else {
            return Ok(report);
        };
        after_turn = last.turn_id;
        let keys: Vec<FactKey> = batch
            .iter()
            .map(|t| FactKey {
                session_id: t.session_id,
                turn_id: t.turn_id,
                as_of: t.closed_at,
            })
            .collect();
        report.turns += keys.len() as u64;
        let rows = compute(persist, &keys)
            .await?
            .into_iter()
            .map(write_row)
            .collect::<Result<Vec<_>, _>>()?;
        report.count(&rows);
        if execute {
            report.written += persist.write_steering_facts(&rows).await?;
        }
    }
}

pub async fn for_closed_turns(
    persist: &PersistHandle,
    session_ids: &[i64],
) -> Result<FillReport, PersistError> {
    let filter = StaleFilter {
        session_ids: session_ids.to_vec(),
        ..StaleFilter::default()
    };
    fill(persist, filter, true).await
}
