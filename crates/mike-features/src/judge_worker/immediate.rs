use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::circuit_breaker::CircuitBreaker;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::logged::ok_or_warn;
use crate::settings::SettingsOverlay;
use crate::store::models::ProxyEvent;
use crate::store::persistence::PersistHandle;

use super::live_judge::{LiveJudge, announce_bundle, judge_and_promote, load_live_bundle};
use super::live_shared::LiveJudgeShared;
use super::turn_sweep::reserve_claim_batch;

const PER_SESSION_LIMIT: i64 = 5;

pub struct ImmediateJudge {
    pub persist: PersistHandle,
    pub llm: OpenRouterClient,
    pub key_slot: crate::llm::key::OpenRouterKeySlot,
    pub toggles: Arc<ArcSwap<FeatureToggles>>,
    pub settings: Arc<ArcSwap<SettingsOverlay>>,
    pub shared: LiveJudgeShared,
    pub judge_cfg: crate::config::steerer::JudgeConfig,
    pub shutdown: CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmediateTarget {
    pub bundle_id: i64,
    pub delay: Duration,
}

pub fn immediate_target(
    judge_worker_on: bool,
    overlay: &SettingsOverlay,
    human_prompt: &str,
) -> Option<ImmediateTarget> {
    if !judge_worker_on || !overlay.turn_judge_immediate || overlay.turn_judge_live_bundle_id <= 0 {
        return None;
    }
    if !crate::wire::genuine::is_genuine_text(human_prompt) {
        return None;
    }
    Some(ImmediateTarget {
        bundle_id: overlay.turn_judge_live_bundle_id,
        delay: Duration::from_millis(
            u64::try_from(overlay.turn_judge_immediate_delay_ms).unwrap_or(0),
        ),
    })
}

impl ImmediateJudge {
    pub fn target_for(&self, event: &ProxyEvent) -> Option<(String, ImmediateTarget)> {
        let ProxyEvent::OperatorTurnClosed {
            session_id,
            human_prompt,
            ..
        } = event
        else {
            return None;
        };
        let target = immediate_target(
            self.toggles.load().judge_worker,
            &self.settings.load(),
            human_prompt,
        )?;
        Some((session_id.clone(), target))
    }

    pub async fn judge_session(
        &self,
        session_uuid: &str,
        target: ImmediateTarget,
        breaker: &mut CircuitBreaker,
    ) -> usize {
        self.try_judge_session(session_uuid, target, breaker)
            .await
            .unwrap_or(0)
    }

    async fn claim_session(&self, session_uuid: &str, bundle_id: i64) -> Option<Vec<i64>> {
        let session_db_id = ok_or_warn(
            self.persist.resolve_session_db_id(session_uuid).await,
            "immediate_judge.session_lookup",
        )??;
        ok_or_warn(
            crate::dossier::worker::build_for_sessions(&self.persist, &[session_db_id]).await,
            "immediate_judge.turn_facts_build",
        );
        let ids = ok_or_warn(
            self.persist
                .claim_live_candidates_for_session(session_db_id, bundle_id, PER_SESSION_LIMIT)
                .await,
            "immediate_judge.claim",
        )?;
        (!ids.is_empty()).then_some(ids)
    }

    fn live_judge<'a>(
        &'a self,
        llm: &'a OpenRouterClient,
        bundle: &'a super::live_judge::LiveBundle,
    ) -> LiveJudge<'a> {
        let overlay = self.settings.load();
        LiveJudge {
            persist: &self.persist,
            llm,
            shared: &self.shared,
            bundle,
            threshold: overlay
                .judge_worker_confidence_threshold
                .unwrap_or(self.judge_cfg.confidence_threshold),
            session_judge_bundle_id: overlay.session_judge_live_bundle_id,
            backoff_base_secs: self.judge_cfg.backoff_base_secs,
            backoff_max_secs: self.judge_cfg.backoff_max_secs,
            jev: crate::llm::decisions::point::Decider::for_point(
                llm,
                &self.persist,
                self.toggles.load().decisions,
                &overlay,
                crate::analysis::judge::jev::POINT,
            ),
        }
    }

    async fn try_judge_session(
        &self,
        session_uuid: &str,
        target: ImmediateTarget,
        breaker: &mut CircuitBreaker,
    ) -> Option<usize> {
        let ids = self.claim_session(session_uuid, target.bundle_id).await?;
        let judged = self.judge_claimed(&ids, target, breaker).await;
        if judged.is_none() {
            ok_or_warn(
                self.persist.release_judge_leases(&ids).await,
                "immediate_judge.release_leases",
            );
        }
        judged
    }

    async fn judge_claimed(
        &self,
        ids: &[i64],
        target: ImmediateTarget,
        breaker: &mut CircuitBreaker,
    ) -> Option<usize> {
        let llm = crate::llm::key::keyed_client(&self.llm, &self.key_slot)?;
        let bundle = load_live_bundle(&self.persist, target.bundle_id).await?;
        announce_bundle(&self.persist, &self.shared, &bundle).await;
        let reservation =
            reserve_claim_batch(&llm, false, i64::try_from(ids.len()).unwrap_or(1)).ok()?;
        let judge = self.live_judge(&llm, &bundle);
        Some(
            judge_and_promote(&judge, &reservation, breaker, ids)
                .await
                .promoted,
        )
    }

    async fn on_event(&self, event: ProxyEvent, breaker: &mut CircuitBreaker) -> bool {
        let Some((session_uuid, target)) = self.target_for(&event) else {
            return false;
        };
        if !self
            .persist
            .holds_llm_role(crate::llm::LlmRole::JudgeWorker, Duration::ZERO)
            .await
        {
            return false;
        }
        tokio::select! {
            biased;
            _ = self.shutdown.cancelled() => return true,
            _ = tokio::time::sleep(target.delay) => {}
        }
        self.judge_session(&session_uuid, target, breaker).await;
        false
    }

    pub async fn run(self, mut rx: broadcast::Receiver<ProxyEvent>) {
        let mut breaker = CircuitBreaker::new(
            self.judge_cfg.circuit_breaker_threshold,
            Duration::from_secs(self.judge_cfg.circuit_breaker_cooldown_base_secs),
            Duration::from_secs(self.judge_cfg.circuit_breaker_cooldown_max_secs),
        );
        loop {
            let received = tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => return,
                received = rx.recv() => received,
            };
            match received {
                Ok(event) => {
                    if self.on_event(event, &mut breaker).await {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        skipped,
                        "immediate judge: event bus lagged; the sweep catches the skipped turns"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::immediate_target;
    use crate::settings::SettingsOverlay;

    fn overlay(bundle: i64, immediate: bool) -> SettingsOverlay {
        SettingsOverlay {
            turn_judge_live_bundle_id: bundle,
            turn_judge_immediate: immediate,
            ..SettingsOverlay::default()
        }
    }

    #[test]
    fn machine_text_prompt_is_ignored() {
        let o = overlay(3, true);
        assert!(
            immediate_target(
                true,
                &o,
                "[SYSTEM NOTIFICATION - NOT USER INPUT] a task finished"
            )
            .is_none()
        );
        assert!(immediate_target(true, &o, "please add the retry and run the tests").is_some());
    }

    #[test]
    fn live_bundle_zero_disables_handler() {
        assert!(immediate_target(true, &overlay(0, true), "please add the retry").is_none());
        assert!(immediate_target(true, &overlay(3, false), "please add the retry").is_none());
        assert!(immediate_target(false, &overlay(3, true), "please add the retry").is_none());
    }

    #[test]
    fn the_delay_comes_from_the_setting() {
        let o = SettingsOverlay {
            turn_judge_immediate_delay_ms: 900,
            ..overlay(3, true)
        };
        let target = immediate_target(true, &o, "please add the retry");
        assert_eq!(target.map(|t| t.delay.as_millis()), Some(900));
    }
}
