use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};

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
        if encoded.len() > limits.max_record_bytes {
            return Ok(ObservationApplyOutcome::BudgetRefused);
        }
        if observation.observed_at < Utc::now() - Duration::days(90) {
            return Ok(ObservationApplyOutcome::Expired);
        }
        let fingerprint = Sha256::digest(&encoded).to_vec();
        let mut tx = self.pool().begin().await?;
        sqlx::query!(
            "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts' FOR UPDATE"
        ).fetch_one(&mut *tx).await?;
        let session = sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
            observation.session_id
        )
        .fetch_one(&mut *tx)
        .await?;
        let existing = sqlx::query_scalar!(
            "SELECT fingerprint FROM runtime.observation_receipts WHERE session_id = $1 AND observation_id = $2",
            session, observation.observation_id
        ).fetch_optional(&mut *tx).await?;
        if let Some(existing) = existing {
            return if existing == fingerprint {
                Ok(ObservationApplyOutcome::Replayed)
            } else {
                Err(PersistError::DataInvariant(
                    "observation identity reused with different facts".into(),
                ))
            };
        }
        observation_write::apply(&mut tx, session, observation).await?;
        sqlx::query!(
            "INSERT INTO runtime.observation_receipts (session_id, observation_id, fingerprint, observed_at, expires_at)
             VALUES ($1, $2, $3, $4, now() + interval '1 day')",
            session, observation.observation_id, fingerprint, observation.observed_at
        ).execute(&mut *tx).await?;
        if !observation_budget::admit(&mut tx, session, observation, limits).await? {
            tx.rollback().await?;
            return Ok(ObservationApplyOutcome::BudgetRefused);
        }
        tx.commit().await?;
        Ok(ObservationApplyOutcome::Applied)
    }
}
