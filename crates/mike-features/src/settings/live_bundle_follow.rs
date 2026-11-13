use crate::store::persistence::eval_repo::EvalPersist as _;
use std::sync::Arc;

use super::service::SettingsService;
use crate::change_log::{ChangeSource, current_build};
use crate::llm::provider::{ModelTier, PROVIDER_KEY, slot_for_key};
use crate::store::persistence::eval_bundles::{BuildOrigin, LiveModelSwap, SwapRequest};

#[cfg(test)]
pub mod provider_tests;
#[cfg(test)]
pub mod tests;

pub const TURN_JUDGE_MODEL_KEY: &str = "judge_worker.model";
pub const SESSION_JUDGE_MODEL_KEY: &str = "session_judge.model";

pub fn pointer_key_for_model_key(key: &str) -> Option<&'static str> {
    match key {
        TURN_JUDGE_MODEL_KEY => Some(super::meta_turn_judge::LIVE_BUNDLE_ID_KEY),
        SESSION_JUDGE_MODEL_KEY => Some(super::meta_session_end::LIVE_BUNDLE_ID_KEY),
        _ => None,
    }
}

fn tier_for_model_key(key: &str) -> Option<ModelTier> {
    match key {
        TURN_JUDGE_MODEL_KEY => Some(ModelTier::Standard),
        SESSION_JUDGE_MODEL_KEY => Some(ModelTier::Fast),
        _ => None,
    }
}

pub fn is_provider_key(key: &str) -> bool {
    key == PROVIDER_KEY || slot_for_key(key).is_some()
}

fn own_model_of(overlay: &super::SettingsOverlay, key: &str) -> Option<String> {
    let stored = match key {
        TURN_JUDGE_MODEL_KEY => overlay.judge_worker_model.as_deref(),
        SESSION_JUDGE_MODEL_KEY => overlay.session_judge_model.as_deref(),
        _ => None,
    };
    stored
        .filter(|m| !m.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            super::SETTINGS_META
                .iter()
                .find(|m| m.key == key)
                .and_then(|m| m.default.as_str())
                .map(str::to_owned)
        })
}

pub fn is_pointer_key(key: &str) -> bool {
    key == super::meta_turn_judge::LIVE_BUNDLE_ID_KEY
        || key == super::meta_session_end::LIVE_BUNDLE_ID_KEY
}

fn pointer_value(overlay: &super::SettingsOverlay, pointer_key: &str) -> i64 {
    if pointer_key == super::meta_turn_judge::LIVE_BUNDLE_ID_KEY {
        overlay.turn_judge_live_bundle_id
    } else {
        overlay.session_judge_live_bundle_id
    }
}

impl SettingsService {
    pub fn live_pointer(&self, key: &str) -> Option<i64> {
        if !is_pointer_key(key) {
            return None;
        }
        Some(pointer_value(&self.overlay.load(), key))
    }

    pub async fn follow_model_change(
        self: &Arc<Self>,
        key: &str,
        model: &str,
        source: &ChangeSource,
    ) -> Option<LiveModelSwap> {
        let pointer_key = pointer_key_for_model_key(key)?;
        let overlay = self.overlay.load_full();
        let live_bundle_id = pointer_value(&overlay, pointer_key);
        if live_bundle_id <= 0 || model.trim().is_empty() {
            return None;
        }
        let model = overlay.llm_provider.resolve(tier_for_model_key(key), model);
        let swap = self
            .swap_for_model(live_bundle_id, key, &model, source)
            .await?;
        let moved = self
            .set_with_source(pointer_key, serde_json::json!(swap.to_bundle_id), source)
            .await;
        if let Err(e) = moved {
            log_pointer_not_moved(&e, pointer_key, swap.to_bundle_id);
        }
        Some(swap)
    }

    pub async fn follow_provider_change(
        self: &Arc<Self>,
        source: &ChangeSource,
    ) -> Vec<LiveModelSwap> {
        let mut swaps = Vec::new();
        for key in [TURN_JUDGE_MODEL_KEY, SESSION_JUDGE_MODEL_KEY] {
            let own = own_model_of(&self.overlay.load(), key);
            let Some(own) = own else {
                continue;
            };
            if let Some(swap) = self.follow_model_change(key, &own, source).await {
                swaps.push(swap);
            }
        }
        swaps
    }

    async fn swap_for_model(
        &self,
        live_bundle_id: i64,
        key: &str,
        model: &str,
        source: &ChangeSource,
    ) -> Option<LiveModelSwap> {
        let build = current_build();
        let source_json = source
            .clone()
            .with_detail("follows", serde_json::json!(key))
            .to_setting_json(&build);
        let request = SwapRequest {
            live_bundle_id,
            model,
            setting_key: key,
            build: BuildOrigin {
                sha: build.sha,
                dirty: build.dirty,
            },
            source_json: &source_json,
        };
        match self.persist.eval().swap_live_bundle_model(request).await {
            Ok(swap) => swap,
            Err(e) => {
                log_swap_failed(&e, key, live_bundle_id);
                None
            }
        }
    }

    pub async fn retire_previous_live_bundle(
        &self,
        previous: i64,
        next: i64,
        source: &ChangeSource,
    ) {
        if previous <= 0 || previous == next {
            return;
        }
        let source_json = source.clone().to_setting_json(&current_build());
        let outcome = self
            .persist
            .eval()
            .retire_bundle(previous, next, &source_json)
            .await;
        log_retire_outcome(outcome, previous, next);
    }
}

fn log_swap_failed(e: &crate::store::persistence::PersistError, key: &str, live_bundle_id: i64) {
    tracing::warn!(
        error = %e,
        key,
        live_bundle_id,
        "live bundle: model swap failed; the live judge keeps its model"
    );
}

fn log_pointer_not_moved(e: &super::service::SettingsServiceError, pointer_key: &str, bundle: i64) {
    tracing::warn!(
        error = %e,
        pointer_key,
        bundle,
        "live bundle: copied for the new model but the pointer did not move"
    );
}

fn log_retire_outcome(
    outcome: Result<bool, crate::store::persistence::PersistError>,
    previous: i64,
    next: i64,
) {
    let message = match outcome {
        Ok(true) => "live bundle: previous bundle retired".to_owned(),
        Ok(false) => return,
        Err(e) => format!("live bundle: previous bundle not retired: {e}"),
    };
    tracing::info!(previous, next, "{message}");
}
