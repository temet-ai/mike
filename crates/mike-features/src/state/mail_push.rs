use uuid::Uuid;

use crate::base_state::{MailPush, PushFuture};
use crate::proxy::remote_control::inject::{
    PushDeps, deliver_or_enqueue, try_push_via_remote_control,
};
use crate::store::models::SystemReminder;

pub struct ProxyMailPush {
    deps: PushDeps,
}

impl ProxyMailPush {
    pub fn new(deps: PushDeps) -> Self {
        Self { deps }
    }
}

impl MailPush for ProxyMailPush {
    fn push_via_remote_control<'a>(
        &'a self,
        recipient: Uuid,
        reminder: &'a SystemReminder,
    ) -> PushFuture<'a> {
        Box::pin(try_push_via_remote_control(&self.deps, recipient, reminder))
    }

    fn deliver_or_enqueue(&self, recipient: Uuid, reminder: SystemReminder) -> PushFuture<'_> {
        Box::pin(deliver_or_enqueue(&self.deps, recipient, reminder))
    }

    fn push_or_queue(&self, recipient: Uuid, reminder: SystemReminder) -> PushFuture<'_> {
        Box::pin(async move {
            if try_push_via_remote_control(&self.deps, recipient, &reminder).await {
                return true;
            }
            let mut store = self.deps.store.write().await;
            crate::injection::enqueue_if_admitted(
                &self.deps.gate,
                &mut store,
                &recipient.to_string(),
                reminder,
            );
            false
        })
    }
}
