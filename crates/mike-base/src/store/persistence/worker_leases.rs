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
const TIMEOUT_IN_TICKS: u32 = 3;
const RECHECK_FLOOR: Duration = Duration::from_secs(5);

pub fn lease_timeout(loop_interval: Duration) -> Duration {
    MIN_LEASE_TIMEOUT.max(loop_interval.saturating_mul(TIMEOUT_IN_TICKS))
}

pub fn renew_after(timeout: Duration) -> Duration {
    timeout / 2
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Verdict {
    held: bool,
    fresh_for: Duration,
}

impl Verdict {
    fn claimed(held: bool, timeout: Duration) -> Self {
        let fresh_for = if held {
            renew_after(timeout)
        } else {
            RECHECK_FLOOR
        };
        Self { held, fresh_for }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeaseHolder {
    pub machine_uuid: Uuid,
    pub age: Duration,
}

fn wait_for_holder(
    holder: Option<&LeaseHolder>,
    machine: Uuid,
    timeout: Duration,
) -> Option<Verdict> {
    let holder = holder.filter(|h| h.machine_uuid != machine && h.age < timeout)?;
    Some(Verdict {
        held: false,
        fresh_for: timeout.saturating_sub(holder.age).max(RECHECK_FLOOR),
    })
}

#[derive(Debug, Clone, Copy)]
struct Decision {
    until: Instant,
    held: bool,
}

fn decision_is_fresh(decision: Decision, now: Instant) -> bool {
    now < decision.until
}

#[derive(Debug, Clone, Copy)]
struct Attempt {
    finished: Instant,
    held: Option<bool>,
}

type ClaimGate = Arc<Mutex<Option<Attempt>>>;

#[derive(Debug, Clone, Default)]
pub struct LeaseMemo {
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

    fn last_held(&self, role: &'static str) -> Option<bool> {
        self.decisions.get(role).map(|decision| decision.held)
    }

    fn remember(&self, role: &'static str, verdict: Verdict, now: Instant) {
        let decision = Decision {
            until: now + verdict.fresh_for,
            held: verdict.held,
        };
        self.decisions.insert(role, decision);
    }

    fn claim_gate(&self, role: &'static str) -> ClaimGate {
        self.claims.entry(role).or_default().clone()
    }

    async fn decide<F, Fut>(&self, role: &'static str, claim: F) -> Option<bool>
    where
        F: FnOnce(Option<bool>) -> Fut,
        Fut: Future<Output = Option<Verdict>>,
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
        let verdict = claim(self.last_held(role)).await;
        let finished = Instant::now();
        if let Some(verdict) = verdict {
            self.remember(role, verdict, finished);
        }
        let held = verdict.map(|verdict| verdict.held);
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

    pub async fn llm_machine_uuid(&self) -> Option<Uuid> {
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

    pub async fn llm_worker_lease_holder(
        &self,
        role: &str,
    ) -> Result<Option<LeaseHolder>, PersistError> {
        crate::metrics::time_db("llm_worker_lease_holder", async move {
            let row = sqlx::query!(
                r#"SELECT machine_uuid,
                          GREATEST(EXTRACT(EPOCH FROM now() - heartbeat_at), 0)::float8 AS "age_secs!"
                     FROM llm.worker_leases
                    WHERE role = $1"#,
                role,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| LeaseHolder {
                machine_uuid: row.machine_uuid,
                age: Duration::try_from_secs_f64(row.age_secs).unwrap_or(Duration::MAX),
            }))
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
        if crate::llm::claude::limit::GATE.holding(Instant::now()) {
            tracing::debug!(
                role = name,
                "llm worker lease: this machine's Claude usage limit is reached; leaving the lease for another machine"
            );
            return false;
        }
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
        let timeout = lease_timeout(loop_interval);
        self.llm_leases
            .decide(role, |last_held| {
                self.claim_or_skip(role, machine, timeout, last_held)
            })
            .await
    }

    async fn claim_or_skip(
        &self,
        role: &'static str,
        machine: Uuid,
        timeout: Duration,
        last_held: Option<bool>,
    ) -> Option<Verdict> {
        if last_held != Some(true) {
            let holder = self
                .llm_worker_lease_holder(role)
                .await
                .inspect_err(|error| {
                    tracing::warn!(
                        role,
                        %error,
                        "llm worker lease: could not read the holder; skipping this tick"
                    );
                })
                .ok()?;
            if let Some(wait) = wait_for_holder(holder.as_ref(), machine, timeout) {
                return Some(wait);
            }
        }
        match self.claim_llm_worker_lease(role, machine, timeout).await {
            Ok(held) => Some(Verdict::claimed(held, timeout)),
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
