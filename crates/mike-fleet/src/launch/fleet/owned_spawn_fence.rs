use crate::store::persistence::fleet_repo::FleetPersist as _;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;

#[derive(Clone, Copy, Debug)]
pub enum Allowed {
    Running,
    Starting,
}

impl Allowed {
    fn state(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Starting => "starting",
        }
    }
}

#[derive(Clone)]
pub struct Identity {
    pub persist: PersistHandle,
    pub fleet: Uuid,
    pub run: String,
    pub allowed: Allowed,
}

pub struct SpawnFence {
    transaction: Transaction<'static, Postgres>,
}

#[cfg(test)]
tokio::task_local! {
    pub static FENCE_TIMEOUT_OVERRIDE: std::time::Duration;
}

pub fn fence_timeout() -> std::time::Duration {
    #[cfg(test)]
    if let Ok(timeout) = FENCE_TIMEOUT_OVERRIDE.try_with(|timeout| *timeout) {
        return timeout;
    }
    std::time::Duration::from_secs(10)
}

impl Identity {
    pub async fn lock(&self) -> Result<SpawnFence, String> {
        tokio::time::timeout(fence_timeout(), self.lock_inner())
            .await
            .map_err(|_| "managed spawn fence timed out".to_string())?
    }

    async fn lock_inner(&self) -> Result<SpawnFence, String> {
        let (transaction, locked) = self
            .persist
            .fleet()
            .begin_fleet_spawn_fence(self.fleet)
            .await
            .map_err(|error| error.to_string())?;
        if !self.accepts(
            locked
                .as_ref()
                .map(|row| (row.state.as_str(), row.run_id.as_deref())),
        ) {
            return Err(
                "managed command refused: fleet state or exact run no longer admits execution"
                    .into(),
            );
        }
        Ok(SpawnFence { transaction })
    }

    async fn activity(&self) -> Activity {
        let row = match self
            .persist
            .fleet()
            .find_fleet(&self.fleet.to_string())
            .await
        {
            Ok(row) => row,
            Err(error) => return Activity::Unknown(error.to_string()),
        };
        if self.accepts(
            row.as_ref()
                .map(|row| (row.state.as_str(), row.run_id.as_deref())),
        ) {
            Activity::Active
        } else {
            Activity::Ended("fleet state or exact run no longer admits execution".into())
        }
    }

    fn accepts(&self, row: Option<(&str, Option<&str>)>) -> bool {
        matches!(row, Some((state, Some(run))) if state == self.allowed.state() && run == self.run)
    }
}

impl SpawnFence {
    pub fn connection(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }

    pub async fn commit(self) -> Result<(), String> {
        tokio::time::timeout(fence_timeout(), self.transaction.commit())
            .await
            .map_err(|_| "managed fence commit outcome unknown after timeout".to_string())?
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Activity {
    Active,
    Ended(String),
    Unknown(String),
}

#[derive(Clone)]
pub enum Authority {
    Durable(Identity),
    #[cfg(any(test, feature = "test-seams"))]
    Fixture(super::managed_test_support::FixtureAuthority),
}

pub enum AuthorityFence {
    Durable(SpawnFence),
    #[cfg(any(test, feature = "test-seams"))]
    Fixture(super::managed_test_support::FixtureFence),
}

impl Authority {
    pub async fn lock(&self) -> Result<AuthorityFence, String> {
        match self {
            Self::Durable(identity) => identity.lock().await.map(AuthorityFence::Durable),
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fixture) => fixture.lock().await.map(AuthorityFence::Fixture),
        }
    }

    pub async fn activity(&self) -> Activity {
        match self {
            Self::Durable(identity) => identity.activity().await,
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fixture) => match fixture.still_active().await {
                Ok(()) => Activity::Active,
                Err(why) => Activity::Ended(why),
            },
        }
    }

    pub fn durable(&self) -> Result<Identity, String> {
        match self {
            Self::Durable(identity) => Ok(identity.clone()),
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(_) => Err("fixture authority cannot create a durable receipt".into()),
        }
    }
}

impl AuthorityFence {
    pub async fn commit(self) -> Result<(), String> {
        match self {
            Self::Durable(fence) => fence.commit().await,
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fence) => fence.commit().await,
        }
    }
}
