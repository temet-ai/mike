use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use uuid::Uuid;

use crate::store::persistence::PersistHandle;

pub type TailFuture<'a> = Pin<Box<dyn Future<Output = Option<Vec<String>>> + Send + 'a>>;

pub trait SessionTail: Send + Sync {
    fn lines<'a>(&'a self, persist: &'a PersistHandle, session: Uuid) -> TailFuture<'a>;
}

static TAIL: OnceLock<Arc<dyn SessionTail>> = OnceLock::new();

pub fn install(tail: Arc<dyn SessionTail>) {
    let _ = TAIL.set(tail);
}

pub async fn session_tail_lines(persist: &PersistHandle, session: Uuid) -> Option<Vec<String>> {
    TAIL.get()?.lines(persist, session).await
}
