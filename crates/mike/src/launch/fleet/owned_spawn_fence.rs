use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;

#[derive(Clone, Copy, Debug)]
pub(super) enum Allowed {
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
pub(super) struct Identity {
    pub persist: PersistHandle,
    pub fleet: Uuid,
    pub run: String,
    pub allowed: Allowed,
}

pub(super) struct SpawnFence {
    transaction: Transaction<'static, Postgres>,
}

impl Identity {
    pub(super) async fn lock(&self) -> Result<SpawnFence, String> {
        tokio::time::timeout(std::time::Duration::from_secs(2), self.lock_inner())
            .await
            .map_err(|_| "managed spawn fence timed out".to_string())?
    }

    async fn lock_inner(&self) -> Result<SpawnFence, String> {
        let (transaction, locked) = self
            .persist
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
        let row = match self.persist.find_fleet(&self.fleet.to_string()).await {
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
    pub(super) fn connection(&mut self) -> &mut PgConnection {
        &mut self.transaction
    }

    pub(super) async fn commit(self) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(2), self.transaction.commit())
            .await
            .map_err(|_| "managed fence commit outcome unknown after timeout".to_string())?
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Activity {
    Active,
    Ended(String),
    Unknown(String),
}

#[derive(Clone)]
pub(super) enum Authority {
    Durable(Identity),
    #[cfg(any(test, feature = "test-seams"))]
    Fixture(super::managed_test_support::FixtureAuthority),
}

pub(super) enum AuthorityFence {
    Durable(SpawnFence),
    #[cfg(any(test, feature = "test-seams"))]
    Fixture(super::managed_test_support::FixtureFence),
}

impl Authority {
    pub(super) async fn lock(&self) -> Result<AuthorityFence, String> {
        match self {
            Self::Durable(identity) => identity.lock().await.map(AuthorityFence::Durable),
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fixture) => fixture.lock().await.map(AuthorityFence::Fixture),
        }
    }

    pub(super) async fn activity(&self) -> Activity {
        match self {
            Self::Durable(identity) => identity.activity().await,
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fixture) => match fixture.still_active().await {
                Ok(()) => Activity::Active,
                Err(why) => Activity::Ended(why),
            },
        }
    }

    pub(super) fn durable(&self) -> Result<Identity, String> {
        match self {
            Self::Durable(identity) => Ok(identity.clone()),
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(_) => Err("fixture authority cannot create a durable receipt".into()),
        }
    }
}

impl AuthorityFence {
    pub(super) async fn commit(self) -> Result<(), String> {
        match self {
            Self::Durable(fence) => fence.commit().await,
            #[cfg(any(test, feature = "test-seams"))]
            Self::Fixture(fence) => fence.commit().await,
        }
    }
}
