use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use super::BUILDER_VERSION;
use super::classify_step::group_streams;
use crate::store::persistence::prompt_stream::{StreamSource, StreamWindow};
use crate::store::persistence::turn_facts::{
    BashUseRecord, SessionRecord, StopFacts, TextFacts, TurnFactsRow, TurnRecord,
};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wire::genuine::stream::StreamMessage;

pub struct Loaded {
    pub streams: HashMap<i64, Vec<StreamMessage>>,
    pub turns: Vec<TurnRecord>,
    pub existing: HashMap<i64, TurnFactsRow>,
    pub sessions: HashMap<i64, SessionRecord>,
    pub max_ids: HashMap<i64, i64>,
}

#[derive(Default)]
pub struct Fresh {
    pub text: HashMap<i64, TextFacts>,
    pub stop: HashMap<i64, StopFacts>,
    pub uses: HashMap<i64, Vec<BashUseRecord>>,
    pub final_text: HashMap<i64, String>,
}

pub async fn load_streams_and_turns(
    persist: &PersistHandle,
    session_ids: &[i64],
    now: DateTime<Utc>,
) -> Result<(HashMap<i64, Vec<StreamMessage>>, Vec<TurnRecord>), PersistError> {
    let window = StreamWindow {
        from: DateTime::<Utc>::UNIX_EPOCH,
        to: now,
    };
    let rows = persist
        .load_prompt_streams(session_ids, window, StreamSource::Stored)
        .await?;
    let turns = persist.load_turn_records(session_ids).await?;
    Ok((group_streams(&rows), turns))
}

pub async fn load_batch(
    persist: &PersistHandle,
    session_ids: &[i64],
    now: DateTime<Utc>,
) -> Result<Loaded, PersistError> {
    let window = StreamWindow {
        from: DateTime::<Utc>::UNIX_EPOCH,
        to: now,
    };
    let rows = persist
        .load_prompt_streams(session_ids, window, StreamSource::Stored)
        .await?;
    let turns = persist.load_turn_records(session_ids).await?;
    let existing = persist
        .load_existing_turn_facts(session_ids)
        .await?
        .into_iter()
        .map(|row| (row.operator_turn_id, row))
        .collect();
    let sessions = persist
        .load_session_records(session_ids)
        .await?
        .into_iter()
        .map(|record| (record.session_id, record))
        .collect();
    let closed: Vec<i64> = turns
        .iter()
        .filter(|turn| turn.closed_at.is_some())
        .map(|turn| turn.turn_id)
        .collect();
    let max_ids = persist.load_max_request_ids(&closed).await?;
    Ok(Loaded {
        streams: group_streams(&rows),
        turns,
        existing,
        sessions,
        max_ids: max_ids.into_iter().collect(),
    })
}

pub fn max_request_id(loaded: &Loaded, turn: &TurnRecord) -> i64 {
    loaded
        .max_ids
        .get(&turn.turn_id)
        .copied()
        .unwrap_or(turn.start_request_id)
}

pub fn needs_fresh(loaded: &Loaded, turn: &TurnRecord) -> bool {
    if turn.closed_at.is_none() {
        return false;
    }
    match loaded.existing.get(&turn.turn_id) {
        None => true,
        Some(row) => {
            row.builder_version != BUILDER_VERSION
                || row.max_request_id < max_request_id(loaded, turn)
        }
    }
}

pub fn settled_stale_final_texts(
    loaded: &Loaded,
    fresh: &Fresh,
    settled_before: DateTime<Utc>,
) -> Vec<i64> {
    loaded
        .turns
        .iter()
        .filter(|turn| turn.closed_at.is_some_and(|at| at <= settled_before))
        .filter(|turn| fresh.text.get(&turn.turn_id).is_some_and(|t| !t.consistent))
        .map(|turn| turn.turn_id)
        .collect()
}

pub async fn load_fresh(
    persist: &PersistHandle,
    loaded: &Loaded,
    session_ids: &[i64],
) -> Result<Fresh, PersistError> {
    let wanted: HashSet<i64> = session_ids.iter().copied().collect();
    let ids: Vec<i64> = loaded
        .turns
        .iter()
        .filter(|turn| wanted.contains(&turn.session_id) && needs_fresh(loaded, turn))
        .map(|turn| turn.turn_id)
        .collect();
    let mut fresh = Fresh::default();
    for text in persist.load_text_facts(&ids).await? {
        fresh.text.insert(text.turn_id, text);
    }
    for stop in persist.load_stop_facts(&ids).await? {
        fresh.stop.insert(stop.turn_id, stop);
    }
    for usage in persist.load_bash_uses(&ids).await? {
        fresh.uses.entry(usage.turn_id).or_default().push(usage);
    }
    for text in persist.load_final_texts(&ids).await? {
        fresh
            .final_text
            .insert(text.turn_id, text.text.unwrap_or_default());
    }
    Ok(fresh)
}
