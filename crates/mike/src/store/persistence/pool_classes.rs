use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sqlx::{ConnectOptions, Connection, PgPool};
use tokio::sync::RwLock;

use super::pool::{self, PoolShape};
use super::{
    ActiveDbTarget, PersistError, PersistHandle, SchemaPolicy, id_cache, judge_ownership,
    migrations, worker_leases,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoolClass {
    Capture,
    Interactive,
    Background,
}

impl PoolClass {
    pub(crate) const ALL: [PoolClass; 3] = [
        PoolClass::Capture,
        PoolClass::Interactive,
        PoolClass::Background,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            PoolClass::Capture => "capture",
            PoolClass::Interactive => "interactive",
            PoolClass::Background => "background",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolClassLimits {
    pub(crate) max_connections: u32,
    pub(crate) acquire_timeout_ms: u64,
    pub(crate) statement_timeout_ms: u64,
}

pub(crate) const DEFAULT_CAPTURE_LIMITS: PoolClassLimits = PoolClassLimits {
    max_connections: 4,
    acquire_timeout_ms: 5_000,
    statement_timeout_ms: 5_000,
};

pub(crate) const DEFAULT_INTERACTIVE_LIMITS: PoolClassLimits = PoolClassLimits {
    max_connections: 6,
    acquire_timeout_ms: 3_000,
    statement_timeout_ms: 15_000,
};

pub(crate) const DEFAULT_BACKGROUND_LIMITS: PoolClassLimits = PoolClassLimits {
    max_connections: 4,
    acquire_timeout_ms: 30_000,
    statement_timeout_ms: 30_000,
};

impl PoolClassLimits {
    fn shape(self) -> PoolShape {
        PoolShape {
            max_connections: self.max_connections,
            acquire_timeout: Duration::from_millis(self.acquire_timeout_ms),
            statement_timeout: Duration::from_millis(self.statement_timeout_ms),
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolClassesConfig {
    pub(crate) capture: PoolClassLimits,
    pub(crate) interactive: PoolClassLimits,
    pub(crate) background: PoolClassLimits,
}

impl Default for PoolClassesConfig {
    fn default() -> Self {
        Self {
            capture: DEFAULT_CAPTURE_LIMITS,
            interactive: DEFAULT_INTERACTIVE_LIMITS,
            background: DEFAULT_BACKGROUND_LIMITS,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PoolClasses {
    capture: PgPool,
    interactive: PgPool,
    background: PgPool,
    split: bool,
}

impl PoolClasses {
    pub(super) fn single(pool: PgPool) -> Self {
        Self {
            capture: pool.clone(),
            interactive: pool.clone(),
            background: pool,
            split: false,
        }
    }

    fn build(database_url: &str, config: &PoolClassesConfig) -> Result<Self, PersistError> {
        Ok(Self {
            capture: lazy_pool(database_url, config.capture)?,
            interactive: lazy_pool(database_url, config.interactive)?,
            background: lazy_pool(database_url, config.background)?,
            split: true,
        })
    }

    pub(crate) fn get(&self, class: PoolClass) -> &PgPool {
        match class {
            PoolClass::Capture => &self.capture,
            PoolClass::Interactive => &self.interactive,
            PoolClass::Background => &self.background,
        }
    }

    pub(crate) fn distinct(&self) -> Vec<(&'static str, &PgPool)> {
        if !self.split {
            return vec![("shared", &self.capture)];
        }
        PoolClass::ALL
            .into_iter()
            .map(|class| (class.label(), self.get(class)))
            .collect()
    }
}

fn lazy_pool(database_url: &str, limits: PoolClassLimits) -> Result<PgPool, PersistError> {
    let shape = limits.shape();
    let options = pool::shaped_connect_options(database_url, &shape)?;
    Ok(pool::shaped_pool_options(&shape).connect_lazy_with(options))
}

async fn probe(database_url: &str, connect_timeout: Duration) -> Result<(), PersistError> {
    let options = pool::shaped_connect_options(database_url, &DEFAULT_CAPTURE_LIMITS.shape())?;
    let mut connection = tokio::time::timeout(connect_timeout, options.connect())
        .await
        .map_err(|_| pool::connect_error(database_url, sqlx::Error::PoolTimedOut))?
        .map_err(|e| pool::connect_error(database_url, e))?;
    pool::require_server_version(&mut connection).await?;
    connection
        .close()
        .await
        .map_err(|e| pool::connect_error(database_url, e))
}

impl PersistHandle {
    pub(super) fn from_pools(pools: PoolClasses) -> Self {
        Self {
            pool: pools.capture.clone(),
            pools,
            active_target: ActiveDbTarget::Remote,
            main_lane_captured: Arc::new(RwLock::new(HashSet::new())),
            judge_ownership_capacity: judge_ownership::JudgeOwnershipCapacity::default(),
            llm_leases: worker_leases::LeaseMemo::default(),
            id_cache: id_cache::IdCache::default(),
            open_changes_memo: Default::default(),
            feed_memos: Default::default(),
        }
    }

    pub(crate) async fn connect_classed(
        database_url: &str,
        connect_timeout: Duration,
        classes: &PoolClassesConfig,
        policy: SchemaPolicy,
    ) -> Result<Self, PersistError> {
        probe(database_url, connect_timeout).await?;
        let pools = PoolClasses::build(database_url, classes)?;
        match policy {
            SchemaPolicy::Migrate => migrations::run_patient(database_url).await?,
            SchemaPolicy::RequireCurrent => migrations::require_current(&pools.capture).await?,
        }
        Ok(Self::from_pools(pools))
    }

    pub(crate) fn on(&self, class: PoolClass) -> Self {
        let mut handle = self.clone();
        handle.pool = self.pools.get(class).clone();
        handle
    }

    pub(crate) fn capture(&self) -> Self {
        self.on(PoolClass::Capture)
    }

    pub(crate) fn background(&self) -> Self {
        self.on(PoolClass::Background)
    }

    pub(crate) fn background_pool(&self) -> &PgPool {
        &self.pools.background
    }

    pub(crate) fn class_pools(&self) -> Vec<(&'static str, &PgPool)> {
        self.pools.distinct()
    }
}

#[cfg(test)]
#[path = "pool_classes_tests.rs"]
mod pool_classes_tests;
