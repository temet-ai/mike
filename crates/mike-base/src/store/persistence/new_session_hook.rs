use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use uuid::Uuid;

use super::{PersistError, PersistHandle};

pub trait NewSessionHook: Send + Sync {
    fn on_new_session<'a>(
        &'a self,
        persist: &'a PersistHandle,
        session: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), PersistError>> + Send + 'a>>;
}

#[derive(Default)]
struct NewSessionHookSlot(OnceLock<Arc<dyn NewSessionHook>>);

impl PersistHandle {
    pub fn set_new_session_hook(&self, hook: Arc<dyn NewSessionHook>) {
        if self.memo::<NewSessionHookSlot>().0.set(hook).is_err() {
            tracing::debug!("a new-session hook was already registered on this handle");
        }
    }

    pub fn new_session_hook(&self) -> Option<Arc<dyn NewSessionHook>> {
        self.memo::<NewSessionHookSlot>().0.get().cloned()
    }
}
