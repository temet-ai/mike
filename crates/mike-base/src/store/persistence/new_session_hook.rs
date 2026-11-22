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

const SET_UP_CAPACITY: usize = 50_000;

#[derive(Default)]
struct SessionsSetUp(dashmap::DashSet<Uuid>);

impl PersistHandle {
    /// Whether this process already stamped the session's machine and ran the
    /// new-session hook for it. The capture store forgets a session once its
    /// in-flight requests finish, so without this every request of a
    /// sequential session would count as new.
    pub fn session_set_up(&self, session_id: &str) -> bool {
        session_id
            .parse::<Uuid>()
            .is_ok_and(|uuid| self.memo::<SessionsSetUp>().0.contains(&uuid))
    }

    pub fn mark_session_set_up(&self, session_id: &str) {
        let Ok(uuid) = session_id.parse::<Uuid>() else {
            return;
        };
        let set = self.memo::<SessionsSetUp>();
        if set.0.len() >= SET_UP_CAPACITY {
            set.0.clear();
        }
        set.0.insert(uuid);
    }
}
