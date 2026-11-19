use std::sync::Arc;

use tokio::sync::RwLock;

use super::chain::Held;
use super::{EarlierTurns, Message, body_messages};
use crate::store::RequestStore;
use crate::store::models::{RequestId, ThreadLink};
use crate::store::persistence::PersistHandle;

const MAX_RESTORE_DEPTH: usize = 256;

pub async fn earlier_turns(
    store: &Arc<RwLock<RequestStore>>,
    session_id: &str,
    body: &[u8],
) -> EarlierTurns {
    let (earlier, link, persist) = {
        let mut s = store.write().await;
        let link = s.link_body(session_id, body);
        let earlier = s.earlier_turns(link.as_ref());
        (earlier, link, s.persist().cloned())
    };
    if !earlier.is_complete()
        && let (Some(link), Some(persist)) = (link.filter(ThreadLink::continues), persist)
    {
        tracing::info!(
            session_id,
            reason_code = if earlier.is_unavailable() {
                "earlier_turns_unavailable"
            } else {
                "earlier_turns_partial"
            },
            previous_message_id = link.previous_message_id.as_deref().unwrap_or(""),
            "earlier turns: the thread chain is not whole in memory; restoring it from the database"
        );
        tokio::spawn(restore(Arc::clone(store), persist, link));
    }
    earlier
}

async fn restore(store: Arc<RwLock<RequestStore>>, persist: PersistHandle, link: ThreadLink) {
    let Some(predecessor) = predecessor_id(&persist, &link).await else {
        return;
    };
    let key = RequestId::from_raw(predecessor as u64);
    if !store.write().await.begin_restore(key) {
        return;
    }
    let restored = held_from_db(&persist, predecessor).await;
    let mut s = store.write().await;
    s.end_restore(key);
    match restored {
        Some(held) => s.remember_held_conversation(key, link.previous_message_id, held),
        None => {
            s.restore_failed(key);
            tracing::debug!(
                request_id = predecessor,
                "earlier turns: the thread chain could not be restored from the database"
            );
        }
    }
}

async fn predecessor_id(persist: &PersistHandle, link: &ThreadLink) -> Option<i64> {
    if let Some(id) = link.predecessor {
        return i64::try_from(id.as_u64()).ok();
    }
    persist
        .request_answered_by(link.previous_message_id.as_deref()?)
        .await
        .ok()
        .flatten()
}

async fn held_from_db(persist: &PersistHandle, newest: i64) -> Option<Held> {
    let mut turns: Vec<Vec<Message>> = Vec::new();
    let mut next = Some(newest);
    let mut complete = true;
    while let Some(id) = next {
        if turns.len() >= MAX_RESTORE_DEPTH {
            complete = false;
            break;
        }
        let step = persist.conversation_step(id).await.ok()??;
        let mut turn = body_messages(&step.body)?;
        turn.push(reply_message(step.reply?)?);
        turns.push(turn);
        next = match ThreadLink::from_body(&step.body) {
            Some(link) if link.continues() || !link.is_known() => {
                let linked = step.predecessor.filter(|_| step.linked && link.continues());
                complete &= linked.is_some();
                linked
            }
            _ => None,
        };
    }
    let messages = turns.into_iter().rev().flatten().collect();
    Some(Held { messages, complete })
}

pub async fn whole_body(
    store: &Arc<RwLock<RequestStore>>,
    session_id: &str,
    body: &[u8],
) -> Option<Vec<u8>> {
    let (earlier, persist) = {
        let mut s = store.write().await;
        let link = s.link_body(session_id, body)?;
        (s.earlier_turns(Some(&link)), s.persist().cloned())
    };
    if earlier.state() == super::EarlierState::Chain {
        return splice(body, earlier);
    }
    whole_body_from_db(&persist?, body).await
}

pub async fn whole_body_from_db(persist: &PersistHandle, body: &[u8]) -> Option<Vec<u8>> {
    let link = ThreadLink::from_body(body).filter(ThreadLink::continues)?;
    let predecessor = predecessor_id(persist, &link).await?;
    let earlier = held_from_db(persist, predecessor).await?;
    splice(body, EarlierTurns::from_chain(earlier.messages))
}

fn splice(body: &[u8], earlier: EarlierTurns) -> Option<Vec<u8>> {
    let mut whole: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(body).ok()?;
    let own = whole.remove("messages")?;
    let mut messages = earlier.values();
    messages.extend(own.as_array()?.iter().cloned());
    whole.insert("messages".to_owned(), serde_json::Value::Array(messages));
    whole.remove("thread");
    serde_json::to_vec(&whole).ok()
}

fn reply_message(reply: serde_json::Value) -> Option<Message> {
    serde_json::value::to_raw_value(&reply).ok().map(Arc::from)
}
