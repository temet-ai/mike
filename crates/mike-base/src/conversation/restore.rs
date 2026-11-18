use std::sync::Arc;

use tokio::sync::RwLock;

use super::{EarlierTurns, Message, Messages, body_messages};
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
    if earlier.is_unavailable()
        && let (Some(link), Some(persist)) = (link, persist)
    {
        tracing::info!(
            session_id,
            reason_code = "earlier_turns_unavailable",
            previous_message_id = link.previous_message_id.as_deref().unwrap_or(""),
            "earlier turns: the predecessor is not held in memory; restoring it from the database"
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
        Some(messages) => s.remember_held_conversation(key, link.previous_message_id, messages),
        None => tracing::debug!(
            request_id = predecessor,
            "earlier turns: the thread chain could not be restored from the database"
        ),
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

async fn held_from_db(persist: &PersistHandle, newest: i64) -> Option<Messages> {
    let mut turns: Vec<Vec<Message>> = Vec::new();
    let mut next = Some(newest);
    while let Some(id) = next {
        if turns.len() >= MAX_RESTORE_DEPTH {
            return None;
        }
        let step = persist.conversation_step(id).await.ok()??;
        let mut turn = body_messages(&step.body)?;
        turn.push(reply_message(step.reply?)?);
        turns.push(turn);
        let continues = ThreadLink::from_body(&step.body).is_some_and(|l| l.continues());
        next = if continues {
            Some(step.predecessor?)
        } else {
            None
        };
    }
    Some(turns.into_iter().rev().flatten().collect())
}

pub async fn whole_body_from_db(persist: &PersistHandle, body: &[u8]) -> Option<Vec<u8>> {
    let link = ThreadLink::from_body(body).filter(ThreadLink::continues)?;
    let predecessor = predecessor_id(persist, &link).await?;
    let earlier = held_from_db(persist, predecessor).await?;
    let mut whole: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(body).ok()?;
    let own = whole.remove("messages")?;
    let mut messages = EarlierTurns::from_chain(earlier).values();
    messages.extend(own.as_array()?.iter().cloned());
    whole.insert("messages".to_owned(), serde_json::Value::Array(messages));
    whole.remove("thread");
    serde_json::to_vec(&whole).ok()
}

fn reply_message(reply: serde_json::Value) -> Option<Message> {
    serde_json::value::to_raw_value(&reply).ok().map(Arc::from)
}
