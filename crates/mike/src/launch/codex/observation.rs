use crate::store::persistence::runtime_repo::RuntimePersist as _;
use chrono::Utc;
use mike_codex::{NotificationObservation, SessionPhase, ThreadUsage, observe_notification};
use serde_json::Value;
use uuid::Uuid;

use crate::runtime::{
    ObservationSource, RuntimeItemObservation, SessionControl, SessionLifecycle,
    SessionObservation, UsageSnapshot,
};
use crate::store::persistence::PersistHandle;

pub(super) struct ManagedObservation {
    persist: PersistHandle,
    session: Uuid,
    thread: String,
    epoch: Uuid,
    sequence: i64,
    incomplete: bool,
}

impl ManagedObservation {
    pub(super) async fn begin(
        persist: PersistHandle,
        session: Uuid,
        thread: &str,
        model: Option<String>,
    ) -> Self {
        let mut observer = Self {
            persist,
            session,
            thread: thread.to_owned(),
            epoch: Uuid::new_v4(),
            sequence: 0,
            incomplete: false,
        };
        let mut initial = observer.next();
        initial.requested_model = model;
        initial.lifecycle = Some(SessionLifecycle::Idle);
        observer.persist(initial).await;
        observer
    }

    pub(super) async fn event(&mut self, event: &Value) {
        let Some(update) = observe_notification(event) else {
            return;
        };
        match update {
            NotificationObservation::Gap { dropped } => {
                self.incomplete = true;
                tracing::warn!(session_id = %self.session, dropped, "Codex observation stream has a gap");
                let observation = self.next();
                self.persist(observation).await;
            }
            NotificationObservation::Session(update) if update.thread_id == self.thread => {
                let mut observation = self.next();
                observation.lifecycle = update.lifecycle.map(lifecycle);
                observation.observed_model = update.observed_model;
                observation.usage = update.usage.map(usage);
                observation.item = update.item.map(|item| RuntimeItemObservation {
                    item_id: item.item_id,
                    turn_id: item.turn_id,
                    kind: item.kind,
                    name: item.name,
                    status: item.status,
                    success: item.success,
                    exit_code: item.exit_code,
                });
                self.persist(observation).await;
            }
            NotificationObservation::Session(_) => {}
        }
    }

    pub(super) async fn end(&mut self) {
        let mut observation = self.next();
        observation.lifecycle = Some(SessionLifecycle::Ended);
        self.persist(observation).await;
    }

    fn next(&mut self) -> SessionObservation {
        self.sequence = self.sequence.saturating_add(1);
        SessionObservation {
            session_id: self.session,
            observation_id: Uuid::new_v5(&self.epoch, &self.sequence.to_be_bytes()),
            runtime: "codex".into(),
            source: ObservationSource {
                kind: "app_server".into(),
                version: None,
                fidelity: "normalized_metadata".into(),
                completeness: if self.incomplete {
                    "gapped"
                } else {
                    "event_fields_only"
                }
                .into(),
            },
            observed_at: Utc::now(),
            sequence: self.sequence,
            lifecycle: None,
            requested_model: None,
            observed_model: None,
            usage: None,
            model_call: None,
            item: None,
            control: Some(SessionControl::Managed),
        }
    }

    async fn persist(&mut self, observation: SessionObservation) {
        use crate::store::persistence::runtime::ObservationApplyOutcome;
        match self
            .persist
            .runtime()
            .apply_session_observation(&observation)
            .await
        {
            Ok(ObservationApplyOutcome::Applied | ObservationApplyOutcome::Replayed) => {}
            other => {
                self.incomplete = true;
                tracing::warn!(session_id = %self.session, outcome = ?other, "Codex session observation not recorded");
            }
        }
    }
}

fn lifecycle(phase: SessionPhase) -> SessionLifecycle {
    match phase {
        SessionPhase::Active => SessionLifecycle::Active,
        SessionPhase::Idle => SessionLifecycle::Idle,
        SessionPhase::Ended => SessionLifecycle::Ended,
        SessionPhase::Failed => SessionLifecycle::Failed,
    }
}

fn usage(usage: ThreadUsage) -> UsageSnapshot {
    UsageSnapshot {
        scope: "thread".into(),
        basis: "provider_cumulative".into(),
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
        total_tokens: usage.total_tokens,
        context_window: usage.context_window,
    }
}
