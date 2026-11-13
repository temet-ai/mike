use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;

pub async fn send(persist: &PersistHandle, from: Uuid, to: Uuid, body: &str) -> bool {
    match persist
        .launch()
        .insert_message(Uuid::new_v4(), to, Some(from), body)
        .await
    {
        Ok(inserted) => inserted.is_some(),
        Err(err) => {
            tracing::warn!(error = %err, %to, "questions: could not mail a question's recipient");
            false
        }
    }
}
