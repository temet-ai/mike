use std::collections::HashMap;

use serde_json::Value;

use super::attribute::{Chain, Sent, Stages};
use super::normalize;
use super::store;
use crate::store::persistence::{PersistError, PersistHandle};

const MAX_CHAIN: usize = 400;

#[derive(Debug, thiserror::Error)]
pub(crate) enum LoadError {
    #[error(transparent)]
    Persist(#[from] PersistError),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("request {0} has no stored body")]
    Missing(i64),
    #[error("request {0}: body is not JSON: {1}")]
    NotJson(i64, serde_json::Error),
}

fn parse(id: i64, bytes: &[u8]) -> Result<Value, LoadError> {
    serde_json::from_slice(bytes).map_err(|e| LoadError::NotJson(id, e))
}

async fn bodies(persist: &PersistHandle, id: i64) -> Result<(Value, Value), LoadError> {
    let raw = u64::try_from(id).unwrap_or_default();
    let sent = persist
        .fetch_request_sent_body_by_id(raw)
        .await?
        .ok_or(LoadError::Missing(id))?;
    let original = persist
        .fetch_request_body_by_id(raw)
        .await?
        .ok_or(LoadError::Missing(id))?;
    Ok((parse(id, &sent)?, parse(id, &original.bytes)?))
}

pub(crate) fn stages_of(manifest: Option<&Value>) -> Stages {
    let list = manifest
        .and_then(|m| m.get("stages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let name = |s: &Value| s.get("stage").and_then(Value::as_str).map(str::to_owned);
    let applied = list
        .iter()
        .filter(|s| s.get("outcome").and_then(Value::as_str) != Some("unchanged"))
        .filter_map(name)
        .collect();
    let filter_reason = list
        .iter()
        .find(|s| s.get("stage").and_then(Value::as_str) == Some("filter"))
        .and_then(|s| s.get("reason_code").and_then(Value::as_str))
        .map(str::to_owned);
    Stages {
        applied,
        filter_reason,
    }
}

async fn sent(persist: &PersistHandle, id: i64) -> Result<Sent, LoadError> {
    let (sent, original) = bodies(persist, id).await?;
    Ok(Sent {
        request_id: id,
        sent,
        original,
        stages: Stages::default(),
    })
}

async fn with_stages(persist: &PersistHandle, mut all: Vec<&mut Sent>) -> Result<(), LoadError> {
    let ids: Vec<i64> = all.iter().map(|s| s.request_id).collect();
    let manifests: HashMap<i64, Option<Value>> = store::manifests(persist.pool(), &ids)
        .await?
        .into_iter()
        .collect();
    for s in &mut all {
        s.stages = stages_of(manifests.get(&s.request_id).and_then(Option::as_ref));
    }
    Ok(())
}

async fn chain(persist: &PersistHandle, session_id: i64, prev: i64) -> Result<Chain, LoadError> {
    let mut deltas = Vec::new();
    let mut id = prev;
    for _ in 0..MAX_CHAIN {
        let step = sent(persist, id).await?;
        if !normalize::is_continuation(&step.sent) {
            deltas.reverse();
            return Ok(Chain {
                full: Some(step),
                deltas,
            });
        }
        let link = normalize::previous_message_id(&step.sent).map(str::to_owned);
        deltas.push(step);
        let Some(link) = link else { break };
        match store::linked_request(persist.pool(), session_id, &link).await? {
            Some(next) => id = next,
            None => break,
        }
    }
    deltas.reverse();
    Ok(Chain { full: None, deltas })
}

pub(crate) async fn load(
    persist: &PersistHandle,
    session_id: i64,
    cur: i64,
    prev: i64,
) -> Result<(Sent, Chain), LoadError> {
    let mut current = sent(persist, cur).await?;
    if normalize::is_continuation(&current.sent) {
        return Ok((current, Chain::default()));
    }
    let mut found = chain(persist, session_id, prev).await?;
    let mut all: Vec<&mut Sent> = found.deltas.iter_mut().collect();
    if let Some(full) = found.full.as_mut() {
        all.push(full);
    }
    all.push(&mut current);
    with_stages(persist, all).await?;
    Ok((current, found))
}
