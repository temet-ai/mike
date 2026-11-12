use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;

use super::{observation_budget, observation_write};
use crate::runtime::SessionObservation;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationApplyOutcome {
    Applied,
    Replayed,
    Expired,
    BudgetRefused,
}

#[derive(Debug, Clone)]
pub struct ObservationLimits {
    pub max_rows: i64,
    pub max_bytes: i64,
    pub max_session_rows: i64,
    pub max_session_bytes: i64,
    pub max_record_bytes: usize,
}

impl Default for ObservationLimits {
    fn default() -> Self {
        Self {
            max_rows: 250_000,
            max_bytes: 128 * 1024 * 1024,
            max_session_rows: 20_000,
            max_session_bytes: 16 * 1024 * 1024,
            max_record_bytes: 16 * 1024,
        }
    }
}

impl PersistHandle {
    pub async fn apply_session_observation(
        &self,
        observation: &SessionObservation,
    ) -> Result<ObservationApplyOutcome, PersistError> {
        self.apply_session_observation_with_limits(observation, &ObservationLimits::default())
            .await
    }

    pub async fn apply_session_observation_with_limits(
        &self,
        observation: &SessionObservation,
        limits: &ObservationLimits,
    ) -> Result<ObservationApplyOutcome, PersistError> {
        observation.validate().map_err(super::invalid)?;
        let encoded = serde_json::to_vec(observation)?;
        if let Some(outcome) = refused_before_write(observation, &encoded, limits) {
            return Ok(outcome);
        }
        let fingerprint = Sha256::digest(&encoded).to_vec();
        let mut tx = self.pool().begin().await?;
        let session = lock_facts_and_resolve_session(&mut tx, observation).await?;
        if let Some(outcome) = replay_outcome(&mut tx, session, observation, &fingerprint).await? {
            return Ok(outcome);
        }
        observation_write::apply(&mut tx, session, observation).await?;
        insert_receipt(&mut tx, session, observation, &fingerprint).await?;
        if !observation_budget::admit(&mut tx, session, observation, limits).await? {
            tx.rollback().await?;
            return Ok(ObservationApplyOutcome::BudgetRefused);
        }
        tx.commit().await?;
        Ok(ObservationApplyOutcome::Applied)
    }
}

fn refused_before_write(
    observation: &SessionObservation,
    encoded: &[u8],
    limits: &ObservationLimits,
) -> Option<ObservationApplyOutcome> {
    if encoded.len() > limits.max_record_bytes {
        return Some(ObservationApplyOutcome::BudgetRefused);
    }
    if observation.observed_at < Utc::now() - Duration::days(90) {
        return Some(ObservationApplyOutcome::Expired);
    }
    None
}

async fn lock_facts_and_resolve_session(
    connection: &mut PgConnection,
    observation: &SessionObservation,
) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts' FOR UPDATE"
    ).fetch_one(&mut *connection).await?;
    sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
        observation.session_id
    )
    .fetch_one(connection)
    .await
}

async fn replay_outcome(
    connection: &mut PgConnection,
    session: i64,
    observation: &SessionObservation,
    fingerprint: &[u8],
) -> Result<Option<ObservationApplyOutcome>, PersistError> {
    let existing = sqlx::query_scalar!(
        "SELECT fingerprint FROM runtime.observation_receipts WHERE session_id = $1 AND observation_id = $2",
        session, observation.observation_id
    ).fetch_optional(connection).await?;
    match existing {
        None => Ok(None),
        Some(existing) if existing == fingerprint => Ok(Some(ObservationApplyOutcome::Replayed)),
        Some(_) => Err(PersistError::DataInvariant(
            "observation identity reused with different facts".into(),
        )),
    }
}

async fn insert_receipt(
    connection: &mut PgConnection,
    session: i64,
    observation: &SessionObservation,
    fingerprint: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO runtime.observation_receipts (session_id, observation_id, fingerprint, observed_at, expires_at)
             VALUES ($1, $2, $3, $4, now() + interval '1 day')",
        session, observation.observation_id, fingerprint, observation.observed_at
    ).execute(connection).await?;
    Ok(())
}
