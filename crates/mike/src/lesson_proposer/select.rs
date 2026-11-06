use chrono::{Duration, Utc};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::proposals::CorrectionCandidate;

use super::ProposerSettings;

pub const CANDIDATE_LOOKBACK_DAYS: i64 = 30;
const FETCH_MULTIPLIER: i64 = 5;
const FETCH_FLOOR: i64 = 200;
const FETCH_CEILING: i64 = 2_000;

fn fetch_limit(max_per_day: i64) -> i64 {
    max_per_day
        .saturating_mul(FETCH_MULTIPLIER)
        .clamp(FETCH_FLOOR, FETCH_CEILING)
}

pub async fn select_candidates(
    persist: &PersistHandle,
    settings: &ProposerSettings,
) -> Result<Vec<CorrectionCandidate>, crate::store::persistence::PersistError> {
    let since = Utc::now() - Duration::days(CANDIDATE_LOOKBACK_DAYS);
    persist
        .select_correction_candidates(since, fetch_limit(settings.max_per_day))
        .await
}
