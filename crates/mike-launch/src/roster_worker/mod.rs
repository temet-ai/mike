use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

use crate::base_state::BaseState;
use crate::llm::side_call::SideCallChain;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist;
use crate::store::persistence::roster_side_call::{RosterCandidate, RosterWindow, SideCallRoster};

pub mod prompt;

const LEASE_ROLE: &str = "side_call_roster";
const PASS_EVERY: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const FAILURE_BACKOFF_MINUTES: i64 = 30;
const FALLBACK_CALLS_PER_HOUR: u32 = 12;
const TURNS_READ: i64 = 3;
const TURN_CHARS: i32 = 1500;
const WINDOW: RosterWindow = RosterWindow {
    lookback_hours: 6,
    settle_secs: 120,
    min_gap_minutes: 15,
    limit: 4,
};

pub fn spawn_roster_worker(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("side_call_roster: no persistence — not spawning");
        return;
    };
    let worker = Worker {
        persist,
        state: state.clone(),
        chain: None,
        backoff: HashMap::new(),
    };
    state.tracker.spawn(worker.run(state.shutdown()));
    tracing::info!("side_call_roster: spawned");
}

struct Worker {
    persist: PersistHandle,
    state: BaseState,
    chain: Option<(Vec<String>, Arc<SideCallChain>)>,
    backoff: HashMap<i64, DateTime<Utc>>,
}

impl Worker {
    async fn run(mut self, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(PASS_EVERY) => {}
            }
            if !self.state.feature_toggle_overlay.load().side_calls {
                continue;
            }
            if !self.persist.holds_worker_role(LEASE_ROLE, PASS_EVERY).await {
                continue;
            }
            self.pass().await;
        }
    }

    fn chain(&mut self) -> Arc<SideCallChain> {
        let models = self.state.settings_overlay.load().side_call_models.clone();
        if let Some((built_for, chain)) = &self.chain
            && *built_for == models
        {
            return chain.clone();
        }
        let chain = Arc::new(
            SideCallChain::from_models(&models, |model| build_client(&self.state, model))
                .with_max_fallback_calls_per_hour(FALLBACK_CALLS_PER_HOUR),
        );
        self.chain = Some((models, chain.clone()));
        chain
    }

    async fn pass(&mut self) {
        let candidates = match self.persist.launch().roster_candidates(WINDOW).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %e, "side_call_roster: candidate query failed");
                return;
            }
        };
        let now = Utc::now();
        self.backoff.retain(|_, until| *until > now);
        let chain = self.chain();
        for candidate in candidates {
            if self.backoff.contains_key(&candidate.session_id) {
                continue;
            }
            if let Err(stop) = self.refresh(&chain, &candidate).await {
                self.backoff.insert(
                    candidate.session_id,
                    Utc::now() + chrono::Duration::minutes(FAILURE_BACKOFF_MINUTES),
                );
                if stop {
                    break;
                }
            }
        }
    }

    async fn refresh(
        &self,
        chain: &SideCallChain,
        candidate: &RosterCandidate,
    ) -> Result<(), bool> {
        let turns = self
            .persist
            .launch()
            .roster_recent_turns(candidate.session_id, TURNS_READ, TURN_CHARS)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "side_call_roster: turn read failed");
                false
            })?;
        if turns.is_empty() {
            return Ok(());
        }
        let digest = prompt::digest(candidate, &turns);
        let outcome = chain
            .verdict::<prompt::RosterAnswer>(prompt::SYSTEM_PROMPT, &digest, prompt::schema())
            .await
            .map_err(|e| {
                tracing::info!(
                    session_id = %candidate.session_uuid,
                    error = %e,
                    "side_call_roster: no model answered; backing off this session"
                );
                true
            })?;
        let Some(fields) = prompt::clean(outcome.value) else {
            return Err(false);
        };
        self.write(candidate, &fields, &outcome.model).await
    }

    async fn write(
        &self,
        candidate: &RosterCandidate,
        fields: &prompt::RosterFields,
        model: &str,
    ) -> Result<(), bool> {
        let written = self
            .persist
            .write_side_call_roster(SideCallRoster {
                session_id: candidate.session_id,
                read_through: candidate.last_closed_at,
                goal: &fields.goal,
                next_steps: &fields.next_steps,
                display_name: &fields.name,
            })
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "side_call_roster: write failed");
                false
            })?;
        tracing::info!(
            session_id = %candidate.session_uuid,
            model,
            written,
            goal_was_ours = candidate.goal_is_ours(),
            "side_call_roster: roster refreshed"
        );
        Ok(())
    }
}

fn build_client(state: &BaseState, model: &str) -> Option<OpenRouterClient> {
    let config = state.config();
    let base = OpenRouterClient::new(
        String::new(),
        model.to_owned(),
        Some(config.steerer.openrouter_base_url.clone()),
        Some(CALL_TIMEOUT),
    )
    .ok()?
    .with_attribution(LlmRole::SideCall, state.llm_usage_sink.as_ref());
    crate::llm::key::effective_client(&base, &state.steerer_openrouter_key)
}
