use anyhow::{Context, Result};
use chrono::{Days, NaiveDate, Utc};

use crate::store::persistence::PersistHandle;

pub(super) async fn rollup_action_effects(
    persist: &PersistHandle,
    for_day: NaiveDate,
) -> Result<u64> {
    let rollup_id = persist
        .current_or_claimed_action_effect_rollup_id(for_day)
        .await
        .with_context(|| format!("claiming the action-effect rollup for {for_day} failed"))?;
    persist
        .write_action_effect_rows(
            rollup_id,
            crate::eval_metrics::day_start(
                for_day.checked_add_days(Days::new(1)).unwrap_or(for_day),
            )
            .min(Utc::now()),
        )
        .await
        .with_context(|| format!("writing action_effect rows for {for_day} failed"))
}
