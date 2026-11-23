use super::group::grouping_similarity;
use super::items::Env;
use super::llm::Failure;
use super::tally::{Roles, Tally};
use crate::store::persistence::consolidation::MeasureInput;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

pub const FLOOR: f32 = 0.62;
pub const CEILING: f32 = 0.80;
pub const STEP: f32 = 0.01;
pub const TOO_LOOSE_ABOVE: f32 = 0.15;
pub const TOO_TIGHT_ABOVE: f32 = 0.30;

pub fn share(part: u32, whole: u32) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

pub fn steer(current: f32, too_loose: f32, too_tight: f32) -> f32 {
    let loose = too_loose > TOO_LOOSE_ABOVE;
    let tight = too_tight > TOO_TIGHT_ABOVE;
    let direction = match (loose, tight) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        (true, true) if too_loose / TOO_LOOSE_ABOVE >= too_tight / TOO_TIGHT_ABOVE => 1.0,
        (true, true) => -1.0,
        (false, false) => 0.0,
    };
    (current + direction * STEP).clamp(FLOOR, CEILING)
}

pub fn next_similarity(current: f32, roles: &Roles) -> (f32, f32) {
    let too_loose = share(roles.elsewhere, roles.members);
    let too_tight = share(roles.covered_outside, roles.covered);
    (steer(current, too_loose, too_tight), too_tight)
}

pub async fn record_measures<L, E>(
    env: &Env<'_, L, E>,
    tally: &Tally,
    run_id: i64,
) -> Result<(), Failure> {
    for (project, roles) in &tally.roles {
        let current = grouping_similarity(env.persist, *project, env.settings).await?;
        let (next, too_tight) = next_similarity(current, roles);
        env.persist
            .knowledge()
            .record_consolidation_measure(MeasureInput {
                run_id,
                project_id: *project,
                split_subject_share: too_tight,
                grouping_distance: next,
            })
            .await
            .map_err(|e| Failure::transient(e.to_string()))?;
    }
    Ok(())
}
