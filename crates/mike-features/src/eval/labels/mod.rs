pub mod import;
pub mod split;
pub mod vocabulary;

pub use crate::web_base::spa_origin::SpaOrigin;
pub use vocabulary::{Axis, Method, SubjectKind, Tier};

use crate::features_settings::meta_eval::{HOLDOUT_PERCENT_KEY, RELAY_ENABLED_KEY};
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn holdout_percent(persist: &PersistHandle) -> Result<i64, PersistError> {
    let value = persist.setting_value(HOLDOUT_PERCENT_KEY).await?;
    Ok(split::clamp_percent(
        value
            .and_then(|v| v.as_i64())
            .unwrap_or(split::DEFAULT_HOLDOUT_PERCENT),
    ))
}

pub async fn relay_enabled(persist: &PersistHandle) -> Result<bool, PersistError> {
    let value = persist.setting_value(RELAY_ENABLED_KEY).await?;
    Ok(value.and_then(|v| v.as_bool()).unwrap_or(true))
}
