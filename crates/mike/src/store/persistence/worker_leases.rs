use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

use super::{PersistError, PersistHandle};
use crate::llm::LlmRole;

#[cfg(test)]
mod tests;

pub const MIN_LEASE_TIMEOUT: Duration = Duration::from_secs(180);
const RENEW_EVERY: Duration = Duration::from_secs(30);
const TIMEOUT_IN_TICKS: u32 = 3;

pub fn lease_timeout(loop_interval: Duration) -> Duration {
    MIN_LEASE_TIMEOUT.max(loop_interval.saturating_mul(TIMEOUT_IN_TICKS))
}

#[derive(Debug, Clone, Copy)]
struct Decision {
    at: Instant,
    held: bool,
}

fn decision_is_fresh(decision: Decision, now: Instant) -> bool {
    now.saturating_duration_since(decision.at) < RENEW_EVERY
}

#[derive(Debug, Clone, Copy)]
struct Attempt {
    finished: Instant,
    held: Option<bool>,
}

type ClaimGate = Arc<Mutex<Option<Attempt>>>;

#[derive(Debug, Clone, Default)]
pub(super) struct LeaseMemo {
    machine: Arc<OnceCell<Uuid>>,
    decisions: Arc<DashMap<&'static str, Decision>>,
    claims: Arc<DashMap<&'static str, ClaimGate>>,
}

impl LeaseMemo {
    fn for_machine(machine: Uuid) -> Self {
        Self {
            machine: Arc::new(OnceCell::new_with(Some(machine))),
            ..Self::default()
        }
    }

    fn remembered(&self, role: &'static str, now: Instant) -> Option<bool> {
        let decision = *self.decisions.get(role)?;
        decision_is_fresh(decision, now).then_some(decision.held)
    }

    fn remember(&self, role: &'static str, held: bool, now: Instant) {
        self.decisions.insert(role, Decision { at: now, held });
    }

    fn claim_gate(&self, role: &'static str) -> ClaimGate {
        self.claims.entry(role).or_default().clone()
    }

    async fn decide<F, Fut>(&self, role: &'static str, claim: F) -> Option<bool>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Option<bool>>,
    {
        let asked = Instant::now();
        if let Some(held) = self.remembered(role, asked) {
            return Some(held);
        }
        let gate = self.claim_gate(role);
        let mut last = gate.lock().await;
        if let Some(attempt) = *last
            && attempt.finished >= asked
        {
            return attempt.held;
        }
        let held = claim().await;
        let finished = Instant::now();
        if let Some(held) = held {
            self.remember(role, held, finished);
        }
        *last = Some(Attempt { finished, held });
        held
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkerLeaseRow {
    pub role: String,
    pub machine_uuid: Uuid,
    pub heartbeat_at: DateTime<Utc>,
    pub age_secs: f64,
}

impl PersistHandle {
    #[must_use]
    pub fn with_llm_machine(mut self, machine: Uuid) -> Self {
        self.llm_leases = LeaseMemo::for_machine(machine);
        self
    }

    pub(crate) async fn llm_machine_uuid(&self) -> Option<Uuid> {
        self.llm_leases
            .machine
            .get_or_try_init(|| async {
                crate::machine_identity::load_default()
                    .await
                    .map(|identity| identity.machine_uuid)
            })
            .await
            .inspect_err(|error| tracing::debug!(%error, "llm worker lease: no machine identity"))
            .ok()
            .copied()
    }

    pub async fn claim_llm_worker_lease(
        &self,
        role: &str,
        machine: Uuid,
        timeout: Duration,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("claim_llm_worker_lease", async move {
            let held = sqlx::query_scalar!(
                r#"WITH claimed AS (
                       INSERT INTO llm.worker_leases AS lease (role, machine_uuid, heartbeat_at)
                       VALUES ($1, $2, now())
                       ON CONFLICT (role) DO UPDATE
                          SET machine_uuid = EXCLUDED.machine_uuid,
                              heartbeat_at = now()
                        WHERE lease.machine_uuid = EXCLUDED.machine_uuid
                           OR lease.heartbeat_at < now() - make_interval(secs => $3::float8)
                    RETURNING 1
                   )
                   SELECT EXISTS (SELECT 1 FROM claimed) AS "held!""#,
                role,
                machine,
                timeout.as_secs_f64(),
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(held)
        })
        .await
    }

    pub async fn list_llm_worker_leases(&self) -> Result<Vec<WorkerLeaseRow>, PersistError> {
        crate::metrics::time_db("list_llm_worker_leases", async move {
            let rows = sqlx::query_as!(
                WorkerLeaseRow,
                r#"SELECT role,
                          machine_uuid,
                          heartbeat_at,
                          EXTRACT(EPOCH FROM now() - heartbeat_at)::float8 AS "age_secs!"
                     FROM llm.worker_leases
                    ORDER BY role"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn holds_llm_role(&self, role: LlmRole, loop_interval: Duration) -> bool {
        let Some(name) = role.as_db_value() else {
            return true;
        };
        self.holds_worker_role(name, loop_interval).await
    }

    pub async fn holds_worker_role(&self, name: &'static str, loop_interval: Duration) -> bool {
        let Some(machine) = self.llm_machine_uuid().await else {
            return true;
        };
        match self.lease_decision(name, machine, loop_interval).await {
            Some(true) => true,
            Some(false) => {
                tracing::debug!(
                    role = name,
                    "llm worker lease held by another machine; skipping this tick"
                );
                false
            }
            None => false,
        }
    }

    async fn lease_decision(
        &self,
        role: &'static str,
        machine: Uuid,
        loop_interval: Duration,
    ) -> Option<bool> {
        self.llm_leases
            .decide(role, || self.claim_or_skip(role, machine, loop_interval))
            .await
    }

    async fn claim_or_skip(
        &self,
        role: &'static str,
        machine: Uuid,
        loop_interval: Duration,
    ) -> Option<bool> {
        match self
            .claim_llm_worker_lease(role, machine, lease_timeout(loop_interval))
            .await
        {
            Ok(held) => Some(held),
            Err(error) => {
                tracing::warn!(
                    role,
                    %error,
                    "llm worker lease: could not claim or renew; skipping this tick"
                );
                None
            }
        }
    }
}
