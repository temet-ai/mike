use serde_json::Value;

use super::ChangesError;
use super::resolve::Resolved;
use crate::change_record::answers;
use crate::store::persistence::PersistHandle;

pub(super) async fn mine(
    persist: &PersistHandle,
    resolved: &Resolved,
) -> Result<Value, ChangesError> {
    let caller = resolved.caller()?;
    let answer = answers::mine(persist, &caller).await?;
    serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))
}
