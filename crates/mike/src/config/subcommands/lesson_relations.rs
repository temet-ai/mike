use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::sync::Arc;

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use tokio_util::task::TaskTracker;

use super::DetectLessonRelationsArgs;
use crate::config::ProxyConfig;
use crate::config::load;
use crate::config::persist_bootstrap::PersistLlmUsageSink;
use crate::llm::provider::ModelTier;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

struct RelationSetup {
    api_key: String,
    proposer_model: String,
    adjudicator_model: String,
    overlay: SettingsOverlay,
}

async fn stored_rows(handle: &PersistHandle) -> Vec<(String, serde_json::Value)> {
    match handle.fetch_settings().await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "detect-lesson-relations: could not read stored settings — using defaults");
            Vec::new()
        }
    }
}

fn apply_stored(
    config: &mut ProxyConfig,
    rows: Vec<(String, serde_json::Value)>,
) -> SettingsOverlay {
    crate::config::apply_settings_overrides(config, &rows);
    crate::settings::apply_db_rows_to_overlay(rows)
}

fn resolve_models(
    config: &ProxyConfig,
    policy: &crate::llm::provider::ProviderPolicy,
) -> Result<(String, String)> {
    let proposer_model = policy.resolve(
        Some(ModelTier::Strong),
        &config.lesson_relations.proposer_model,
    );
    let adjudicator_model = policy.resolve(
        Some(ModelTier::Fast),
        &config.lesson_relations.adjudicator_model,
    );
    crate::lesson_relations::assert_models_differ(&proposer_model, &adjudicator_model).map_err(
        |error| {
            anyhow::anyhow!(
                "detect-lesson-relations: {error}. Change the stored setting lesson_relations.proposer_model (now {:?}) or lesson_relations.adjudicator_model (now {:?}) at /settings so the two differ",
                config.lesson_relations.proposer_model,
                config.lesson_relations.adjudicator_model
            )
        },
    )?;
    Ok((proposer_model, adjudicator_model))
}

fn build_setup(config: &ProxyConfig, overlay: SettingsOverlay) -> Result<RelationSetup> {
    let policy = overlay.llm_provider.clone();
    let (proposer_model, adjudicator_model) = resolve_models(config, &policy)?;
    let proposer_key = crate::llm::key::cli_api_key(
        config.steerer.openrouter_api_key.as_ref(),
        &policy,
        Some(ModelTier::Strong),
        &config.lesson_relations.proposer_model,
    )
    .map_err(|e| anyhow::anyhow!("detect-lesson-relations: {e}"))?;
    let adjudicator_key = crate::llm::key::cli_api_key(
        config.steerer.openrouter_api_key.as_ref(),
        &policy,
        Some(ModelTier::Fast),
        &config.lesson_relations.adjudicator_model,
    )
    .map_err(|e| anyhow::anyhow!("detect-lesson-relations: {e}"))?;
    let api_key = if proposer_key.is_empty() {
        adjudicator_key
    } else {
        proposer_key
    };
    Ok(RelationSetup {
        api_key,
        proposer_model,
        adjudicator_model,
        overlay,
    })
}

pub(super) async fn detect_lesson_relations(
    args: DetectLessonRelationsArgs,
    cli: crate::config::CliOverrides,
) -> Result<()> {
    let mut config =
        load::load(cli).context("failed to resolve config for detect-lesson-relations")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!("detect-lesson-relations requires a database_url");
    };
    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("could not connect to the target database")?;
    let overlay = apply_stored(&mut config, stored_rows(&handle).await);
    if !config.lesson_relations.enabled {
        println!("lesson_relations.enabled is false — not running the detector.");
        return Ok(());
    }
    let setup = build_setup(&config, overlay)?;

    let tracker = TaskTracker::new();
    let sink: Arc<dyn LlmUsageSink> =
        Arc::new(PersistLlmUsageSink::new(handle.clone(), tracker.clone()).await);
    install_governor(&setup.overlay, &sink);

    let outcome = run_detection_with_clients(&config, &handle, &args, &sink, &setup).await;

    tracker.close();
    tracker.wait().await;
    outcome
}

fn install_governor(stored: &SettingsOverlay, sink: &Arc<dyn LlmUsageSink>) {
    let mut overlay = stored.clone();
    overlay.llm_budget_steerer_share_pct = 0;
    overlay.llm_budget_session_judge_share_pct = 0;
    overlay.llm_budget_judge_worker_share_pct = 0;
    overlay.llm_budget_other_share_pct = 0;
    overlay.llm_budget_eval_share_pct = 0;
    overlay.llm_budget_lesson_relations_share_pct = 100;
    let shared = Arc::new(ArcSwap::from_pointee(overlay));
    crate::llm::provider::install(shared.clone());
    crate::llm::limiter::install_runtime(shared, Some(sink.clone()));
}

const PAIR_CALLS_PER_RUN: usize = 150;

async fn run_detection_with_clients(
    config: &ProxyConfig,
    handle: &PersistHandle,
    args: &DetectLessonRelationsArgs,
    sink: &Arc<dyn LlmUsageSink>,
    setup: &RelationSetup,
) -> Result<()> {
    let proposer_client = OpenRouterClient::new(
        setup.api_key.clone(),
        setup.proposer_model.clone(),
        None,
        Some(std::time::Duration::from_secs(
            config.lesson_relations.request_timeout_secs,
        )),
    )?;
    let runtime_settings = crate::settings::resolve_shared_llm_settings(&setup.overlay);
    let proposer_client = proposer_client
        .with_attribution(LlmRole::LessonRelations, Some(sink))
        .with_runtime_settings(runtime_settings);
    let adjudicator_client = proposer_client
        .with_model(setup.adjudicator_model.clone())?
        .with_tier(ModelTier::Fast);
    let proposer = crate::lesson_relations::ModelParticipant {
        client: &proposer_client,
        model: &setup.proposer_model,
    };
    let adjudicator = crate::lesson_relations::ModelParticipant {
        client: &adjudicator_client,
        model: &setup.adjudicator_model,
    };
    let project_ids = if let Some(path) = args.project.as_ref() {
        vec![
            handle
                .lessons()
                .lookup_project_id_by_path(path)
                .await?
                .ok_or_else(|| anyhow::anyhow!("no project registered at path {path:?}"))?,
        ]
    } else {
        handle
            .lessons()
            .list_project_ids_with_embedded_lessons()
            .await?
    };
    let model_judge = crate::lesson_relations::ModelPairJudge {
        proposer: &proposer,
        adjudicator: &adjudicator,
    };
    let decisions_on = handle
        .fetch_feature_toggles()
        .await
        .map(|rows| rows.iter().any(|(key, on, _)| key == "decisions" && *on))
        .unwrap_or(false);
    let env = crate::llm::decisions::env::DecisionEnv::new(
        proposer_client.clone(),
        handle.clone(),
        decisions_on,
        Arc::new(setup.overlay.clone()),
    );
    let pair_jev = crate::lesson_relations::decide::PairJev::new(Some(&env), PAIR_CALLS_PER_RUN);
    let judge = crate::lesson_relations::decide::JevJudge {
        inner: &model_judge,
        jev: pair_jev.as_ref(),
    };
    let reranked = crate::lesson_relations::run_detection_on_reranked(
        handle,
        &judge,
        &project_ids,
        args.dry_run,
    )
    .await?;
    let report = crate::lesson_relations::run_detection(
        handle,
        &judge,
        config.lesson_relations.candidate_distance_threshold,
        &project_ids,
        args.dry_run,
        &reranked.handled,
    )
    .await?;
    println!(
        "reranked pairs considered: {}; relations: {}; candidate pairs considered: {}; relations: {}",
        reranked.detection.candidate_pairs_considered,
        reranked.detection.relations.len(),
        report.candidate_pairs_considered,
        report.relations.len()
    );
    Ok(())
}

#[cfg(test)]
#[path = "lesson_relations_tests.rs"]
mod lesson_relations_tests;
