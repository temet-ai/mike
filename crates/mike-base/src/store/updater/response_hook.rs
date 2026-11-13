use std::future::Future;
use std::pin::Pin;

use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;

pub struct CompletedResponse<'a> {
    pub persist: &'a PersistHandle,
    pub request_id: RequestId,
    pub request_id_i64: i64,
    pub project_path: &'a str,
    pub session_uuid: &'a str,
    pub text: &'a str,
}

pub trait ResponseHook: Send + Sync {
    fn on_response<'a>(
        &'a self,
        response: CompletedResponse<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
