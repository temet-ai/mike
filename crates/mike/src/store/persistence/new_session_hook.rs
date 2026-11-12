//! Work a feature runs when capture writes a session's first request (the
//! steerer enrols its auto-enrolled steerers). Registered once on the handle the
//! app builds; capture calls it without knowing which feature it is.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use uuid::Uuid;

use super::{PersistError, PersistHandle};

pub(crate) trait NewSessionHook: Send + Sync {
    fn on_new_session<'a>(
        &'a self,
        persist: &'a PersistHandle,
        session: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), PersistError>> + Send + 'a>>;
}

#[derive(Default)]
struct NewSessionHookSlot(OnceLock<Arc<dyn NewSessionHook>>);

impl PersistHandle {
    pub(crate) fn set_new_session_hook(&self, hook: Arc<dyn NewSessionHook>) {
        if self.memo::<NewSessionHookSlot>().0.set(hook).is_err() {
            tracing::debug!("a new-session hook was already registered on this handle");
        }
    }

    pub(super) fn new_session_hook(&self) -> Option<Arc<dyn NewSessionHook>> {
        self.memo::<NewSessionHookSlot>().0.get().cloned()
    }
}
