use serde_json::{Value, json};

use super::fetch::Window;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_repo::EvalPersist as _;

pub(super) async fn recorded(handle: &PersistHandle, window: &Window) -> anyhow::Result<Value> {
    let lessons = handle
        .eval()
        .list_lesson_exposures(None, window.start, window.end)
        .await?;
    Ok(json!({
        "from": window.start,
        "until": window.end,
        "lessons": lessons,
        "measure": "recorded_exposure_association",
    }))
}
