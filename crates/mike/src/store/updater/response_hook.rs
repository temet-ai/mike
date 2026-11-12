//! Work a feature runs on every completed response, handed to the updater by
//! the code that spawns it. The memory harvester (memory_harvester::HarvesterHook)
//! is one; the updater itself knows nothing about lessons.

use std::future::Future;
use std::pin::Pin;

use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;

/// What the updater knows about a response once its stream has ended.
pub(crate) struct CompletedResponse<'a> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) request_id: RequestId,
    pub(crate) request_id_i64: i64,
    pub(crate) project_path: &'a str,
    pub(crate) session_uuid: &'a str,
    pub(crate) text: &'a str,
}

pub(crate) trait ResponseHook: Send + Sync {
    fn on_response<'a>(
        &'a self,
        response: CompletedResponse<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
