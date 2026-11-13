use std::sync::Arc;
use std::{future::Future, io};

use sqlx::{Connection, PgConnection};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::{PersistError, PersistHandle, pool};

#[derive(Debug, Clone)]
pub struct JudgeOwnershipCapacity {
    turns: Arc<Semaphore>,
    history: Arc<Semaphore>,
}

impl Default for JudgeOwnershipCapacity {
    fn default() -> Self {
        Self {
            turns: Arc::new(Semaphore::new(2)),
            history: Arc::new(Semaphore::new(1)),
        }
    }
}

pub struct JudgeOwnership {
    connection: PgConnection,
    key: String,
    _permit: OwnedSemaphorePermit,
}

impl JudgeOwnership {
    pub async fn check_connection(&mut self) -> Result<(), PersistError> {
        connection_deadline(self.connection.ping()).await?;
        Ok(())
    }

    pub async fn release(mut self) -> Result<(), PersistError> {
        let query = sqlx::query_scalar!(
            "SELECT pg_advisory_unlock(hashtextextended($1::text, 0)) AS \"unlocked!\"",
            self.key,
        );
        let unlocked = connection_deadline(query.fetch_one(&mut self.connection)).await?;
        if !unlocked {
            return Err(PersistError::DataInvariant(
                "judge ownership was lost before release".to_owned(),
            ));
        }
        connection_deadline(self.connection.close()).await?;
        Ok(())
    }
}

impl PersistHandle {
    pub async fn try_lock_judge_turn(
        &self,
        bundle_id: i64,
        turn_id: i64,
    ) -> Result<Option<JudgeOwnership>, PersistError> {
        self.try_lock_judge(
            format!("mike:judge:turn:{bundle_id}:{turn_id}"),
            self.judge_ownership_capacity.turns.clone(),
        )
        .await
    }

    pub async fn try_lock_judge_history(
        &self,
        run_id: i64,
    ) -> Result<Option<JudgeOwnership>, PersistError> {
        self.try_lock_judge(
            format!("mike:judge:history:{run_id}"),
            self.judge_ownership_capacity.history.clone(),
        )
        .await
    }

    async fn try_lock_judge(
        &self,
        key: String,
        capacity: Arc<Semaphore>,
    ) -> Result<Option<JudgeOwnership>, PersistError> {
        let Ok(permit) = capacity.try_acquire_owned() else {
            return Ok(None);
        };
        let options = self.pool.connect_options();
        let mut connection = connection_deadline(PgConnection::connect_with(&options)).await?;
        let query = sqlx::query_scalar!(
            "SELECT pg_try_advisory_lock(hashtextextended($1::text, 0)) AS \"acquired!\"",
            key,
        );
        let acquired = connection_deadline(query.fetch_one(&mut connection)).await?;
        if !acquired {
            connection_deadline(connection.close()).await?;
            return Ok(None);
        }
        Ok(Some(JudgeOwnership {
            connection,
            key,
            _permit: permit,
        }))
    }
}

async fn connection_deadline<T>(
    operation: impl Future<Output = Result<T, sqlx::Error>>,
) -> Result<T, PersistError> {
    tokio::time::timeout(pool::DEFAULT_ACQUIRE_TIMEOUT, operation)
        .await
        .map_err(|_| {
            sqlx::Error::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "judge ownership connection operation timed out",
            ))
        })?
        .map_err(Into::into)
}

#[cfg(test)]
mod tests;
