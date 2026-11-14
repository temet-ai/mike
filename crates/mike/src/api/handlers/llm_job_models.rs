use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::judge_worker::{resolve_session_judge_runtime, resolve_turn_judge_runtime};
use crate::llm::model_ref::{ModelRef, Provider};
use crate::llm::provider::ModelTier;

pub(crate) const STEERERS: &str = "Steerers";
pub(crate) const TURN_JUDGE: &str = "Turn judge";
pub(crate) const SESSION_JUDGE: &str = "Session judge";
pub(crate) const LESSON_PROPOSER: &str = "Lesson proposer";
pub(crate) const LESSON_RELATIONS: &str = "Lesson relations";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobModel {
    pub(crate) job: &'static str,
    pub(crate) model: ModelRef,
}

fn job(job: &'static str, model: &str) -> JobModel {
    JobModel {
        job,
        model: ModelRef::parse(model),
    }
}

pub(crate) fn steerer_job_models(state: &AppState) -> Vec<JobModel> {
    let overlay = state.settings_overlay.load();
    let policy = &overlay.llm_provider;
    let default = overlay.live_steerer_model(&state.config.steerer.model);
    let registry = state.steerer_registry.load();
    let mut models = vec![job(
        STEERERS,
        &policy.resolve(Some(ModelTier::Strong), &default),
    )];
    models.extend(
        registry
            .steerers
            .iter()
            .filter(|steerer| steerer.enabled && !steerer.model.is_empty())
            .map(|steerer| {
                job(
                    STEERERS,
                    &policy.resolve(Some(ModelTier::Strong), &steerer.model),
                )
            }),
    );
    models
}

pub(crate) fn unique_providers(models: &[JobModel]) -> Vec<Provider> {
    Provider::ALL
        .into_iter()
        .filter(|provider| models.iter().any(|m| m.model.provider == *provider))
        .collect()
}

pub(crate) fn jobs_on(models: &[JobModel], provider: Provider) -> Vec<&'static str> {
    let mut jobs: Vec<&'static str> = Vec::new();
    for entry in models.iter().filter(|m| m.model.provider == provider) {
        if !jobs.contains(&entry.job) {
            jobs.push(entry.job);
        }
    }
    jobs
}

pub(crate) async fn enabled_job_models(
    state: &AppState,
    toggles: &FeatureToggles,
) -> Vec<JobModel> {
    let overlay = state.settings_overlay.load();
    let policy = &overlay.llm_provider;
    let mut models = Vec::new();
    if toggles.steerer {
        models.extend(steerer_job_models(state));
    }
    if toggles.judge_worker {
        let runtime = resolve_turn_judge_runtime(
            &overlay,
            &state.config.steerer.judge,
            &state.config.steerer.model,
        );
        models.push(job(TURN_JUDGE, &runtime.model));
    }
    if toggles.session_judge {
        let session_judge = state
            .config
            .sections
            .get::<crate::features_settings::SessionJudgeConfig>();
        let runtime = resolve_session_judge_runtime(&overlay, &session_judge);
        models.push(job(SESSION_JUDGE, &runtime.model));
    }
    if toggles.judge_lessons
        && let Some(persist) = state.persist()
    {
        let settings = crate::lesson_proposer::ProposerSettings::load(persist).await;
        models.push(job(
            LESSON_PROPOSER,
            &policy.resolve(Some(ModelTier::Fast), &settings.effective_model),
        ));
    }
    let relations = state
        .config
        .sections
        .get::<crate::lessons_settings::LessonRelationsConfig>();
    if relations.enabled {
        models.push(job(
            LESSON_RELATIONS,
            &policy.resolve(Some(ModelTier::Strong), &relations.proposer_model),
        ));
        models.push(job(
            LESSON_RELATIONS,
            &policy.resolve(Some(ModelTier::Fast), &relations.adjudicator_model),
        ));
    }
    models
}

pub(crate) fn configured_model_refs(state: &AppState) -> Vec<ModelRef> {
    let overlay = state.settings_overlay.load();
    let relations = state
        .config
        .sections
        .get::<crate::lessons_settings::LessonRelationsConfig>();
    let mut stored: Vec<String> = vec![
        overlay.live_steerer_model(&state.config.steerer.model),
        state
            .config
            .sections
            .get::<crate::features_settings::SessionJudgeConfig>()
            .model,
        relations.proposer_model.clone(),
        relations.adjudicator_model.clone(),
    ];
    stored.extend(state.config.steerer.judge.model.clone());
    stored.extend(overlay.judge_worker_model.clone());
    stored.extend(overlay.session_judge_model.clone());
    stored.extend(
        state
            .steerer_registry
            .load()
            .steerers
            .iter()
            .map(|steerer| steerer.model.clone()),
    );
    stored
        .iter()
        .map(|model| model.trim())
        .filter(|model| !model.is_empty())
        .map(ModelRef::parse)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(job: &'static str, model: &str) -> JobModel {
        JobModel {
            job,
            model: ModelRef::parse(model),
        }
    }

    #[test]
    fn jobs_on_lists_each_job_once_for_the_provider() {
        let models = [
            entry(STEERERS, "codex/gpt-6-sol"),
            entry(STEERERS, "codex/gpt-6.1-sol"),
            entry(TURN_JUDGE, "google/gemini-3.8-flash"),
            entry(SESSION_JUDGE, "codex/gpt-6-luna"),
        ];
        assert_eq!(
            jobs_on(&models, Provider::Codex),
            vec![STEERERS, SESSION_JUDGE]
        );
        assert_eq!(jobs_on(&models, Provider::OpenRouter), vec![TURN_JUDGE]);
        assert!(jobs_on(&models, Provider::Claude).is_empty());
    }

    #[test]
    fn unique_providers_follow_the_fixed_provider_order() {
        let models = [
            entry(STEERERS, "codex/gpt-6-sol"),
            entry(TURN_JUDGE, "openrouter/anthropic/claude-sonnet-5-5"),
            entry(SESSION_JUDGE, "codex/gpt-6-luna"),
        ];
        assert_eq!(
            unique_providers(&models),
            vec![Provider::OpenRouter, Provider::Codex]
        );
    }
}
