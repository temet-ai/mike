use crate::store::persistence::steerer_repo::SteererPersist as _;

use std::future::Future;
use std::pin::Pin;

use uuid::Uuid;

use crate::store::persistence::new_session_hook::NewSessionHook;
use crate::store::persistence::{PersistError, PersistHandle};

pub struct AutoSteererEnrolment;

impl NewSessionHook for AutoSteererEnrolment {
    fn on_new_session<'a>(
        &'a self,
        persist: &'a PersistHandle,
        session: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), PersistError>> + Send + 'a>> {
        Box::pin(async move {
            persist
                .steerer()
                .enrol_auto_steerers(session)
                .await
                .map(|_| ())
        })
    }
}
