use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::change_log::{ChangeSource, ChangeVia};
use std::sync::Arc;

use crate::search_ranking::tuning::{Attempt, Grade, tune};
use crate::semantic::KnowledgeState;
use crate::semantic::knowledge_state::SettingWriter;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::search_grades::{NewTuningRun, TuningOutcome};

pub const ACTOR: &str = "rerank_tuner";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tuned {
    AlreadyToday,
    TooFewGrades(usize),
    NoSettingsService,
    Recorded { kept: usize, tried: usize },
}

pub async fn tune_if_due(
    state: &KnowledgeState,
    persist: &PersistHandle,
    window_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Tuned, crate::store::persistence::PersistError> {
    if persist.knowledge().search_tuned_since(window_start).await? {
        return Ok(Tuned::AlreadyToday);
    }
    let service = state.rerank_service();
    let config = service.current_config();
    let stored = persist
        .knowledge()
        .search_grades_since(now - Duration::days(config.nightly.lookback_days))
        .await?;
    let grades: Vec<Grade> = stored
        .into_iter()
        .filter_map(|grade| {
            let targets = targets(grade.used_references, grade.relevant_references);
            Grade::from_stored(grade.candidates, targets)
        })
        .filter(|grade| !grade.used.is_empty())
        .collect();
    let graded = i32::try_from(grades.len()).unwrap_or(i32::MAX);
    if grades.len() < config.nightly.min_graded_searches {
        persist
            .knowledge()
            .insert_search_tuning_runs(TuningOutcome::TooFewGrades, &[], graded, now)
            .await?;
        return Ok(Tuned::TooFewGrades(grades.len()));
    }
    let Some(settings) = state.settings() else {
        return Ok(Tuned::NoSettingsService);
    };
    let attempts = tune(
        &grades,
        &service.ranking(),
        config.candidates,
        config.nightly.min_gain,
    );
    let mut runs = Vec::with_capacity(attempts.len());
    for attempt in attempts {
        let run = apply(state, settings, &attempt, grades.len()).await;
        runs.push(run);
    }
    let kept = runs.iter().filter(|run| run.kept).count();
    let tried = runs.len();
    let outcome = if kept > 0 {
        TuningOutcome::Changed
    } else {
        TuningOutcome::NoChange
    };
    persist
        .knowledge()
        .insert_search_tuning_runs(outcome, &runs, graded, now)
        .await?;
    Ok(Tuned::Recorded { kept, tried })
}

pub fn targets(used: Vec<String>, relevant: Vec<String>) -> Vec<String> {
    let mut targets = used;
    for reference in relevant {
        if !targets.contains(&reference) {
            targets.push(reference);
        }
    }
    targets
}

async fn apply(
    state: &KnowledgeState,
    settings: &Arc<dyn SettingWriter>,
    attempt: &Attempt,
    graded_events: usize,
) -> NewTuningRun {
    let mut run = NewTuningRun {
        setting_key: attempt.key.to_owned(),
        before: attempt.before.clone(),
        after: attempt.after.clone(),
        score_before: attempt.score_before,
        score_after: attempt.score_after,
        kept: attempt.kept,
        reason: attempt.reason.clone(),
    };
    if !attempt.kept {
        return run;
    }
    let source = ChangeSource {
        via: ChangeVia::Mike,
        actor: Some(ACTOR.to_owned()),
        request_id: None,
        detail: json!({
            "before": attempt.before,
            "after": attempt.after,
            "score_before": attempt.score_before,
            "score_after": attempt.score_after,
            "graded_events": graded_events,
        }),
    };
    match settings
        .write(attempt.key, attempt.after.clone(), &source)
        .await
    {
        Ok(validated) => {
            state
                .rerank_service()
                .apply_setting(attempt.key, &validated);
        }
        Err(error) => {
            tracing::warn!(key = attempt.key, %error, "rerank_nightly: a tuned value was not accepted");
            run.kept = false;
            run.reason = format!("{}; the setting write failed: {error}", attempt.reason);
        }
    }
    run
}
