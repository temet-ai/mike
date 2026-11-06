use std::sync::Arc;

use tokio::sync::{OwnedRwLockReadGuard, RwLock};

use super::super::owned_spawn_fence::Allowed;

struct FixtureRow {
    state: String,
    run: Option<String>,
}

#[derive(Clone)]
pub(in crate::launch::fleet) struct FixtureAuthority {
    row: Arc<RwLock<FixtureRow>>,
    expected_run: String,
    allowed: Allowed,
    root_guard: Arc<tempfile::TempDir>,
}

pub(in crate::launch::fleet) struct FixtureFence {
    _held: OwnedRwLockReadGuard<FixtureRow>,
    _root_guard: Arc<tempfile::TempDir>,
}

impl FixtureAuthority {
    pub(super) fn new(root_guard: Arc<tempfile::TempDir>, run: &str, allowed: Allowed) -> Self {
        Self {
            row: Arc::new(RwLock::new(FixtureRow {
                state: state(allowed).into(),
                run: Some(run.into()),
            })),
            expected_run: run.into(),
            allowed,
            root_guard,
        }
    }

    pub(in crate::launch::fleet) async fn lock(&self) -> Result<FixtureFence, String> {
        let held = self.row.clone().read_owned().await;
        self.check(&held)?;
        Ok(FixtureFence {
            _held: held,
            _root_guard: self.root_guard.clone(),
        })
    }

    pub(in crate::launch::fleet) async fn still_active(&self) -> Result<(), String> {
        let row = self.row.read().await;
        self.check(&row)
    }

    fn check(&self, row: &FixtureRow) -> Result<(), String> {
        if row.state == state(self.allowed)
            && row.run.as_deref() == Some(self.expected_run.as_str())
        {
            Ok(())
        } else {
            Err("fixture fleet state or exact run no longer admits execution".into())
        }
    }

    pub(in crate::launch::fleet) async fn transition(&self, state: &str, run: Option<&str>) {
        let mut row = self.row.write().await;
        row.state = state.into();
        row.run = run.map(str::to_string);
    }
}

impl FixtureFence {
    pub(in crate::launch::fleet) async fn commit(self) -> Result<(), String> {
        Ok(())
    }
}

fn state(allowed: Allowed) -> &'static str {
    match allowed {
        Allowed::Running => "running",
        Allowed::Starting => "starting",
    }
}

pub(in crate::launch::fleet) async fn fixture_lifecycle_change_waits_for_retained_spawn_fence()
-> anyhow::Result<()> {
    let root = Arc::new(tempfile::tempdir()?);
    let authority = FixtureAuthority::new(root, "exact", Allowed::Running);
    let held = authority.lock().await.map_err(anyhow::Error::msg)?;
    let transition = authority.transition("stopping", Some("exact"));
    tokio::pin!(transition);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut transition)
            .await
            .is_err()
    );
    held.commit().await.map_err(anyhow::Error::msg)?;
    transition.await;
    assert!(authority.still_active().await.is_err());
    assert!(authority.lock().await.is_err());
    Ok(())
}

pub(in crate::launch::fleet) async fn fixture_rejects_a_different_exact_run_and_a_wrong_lifecycle()
-> anyhow::Result<()> {
    let root = Arc::new(tempfile::tempdir()?);
    let authority = FixtureAuthority::new(root, "exact", Allowed::Starting);
    assert!(authority.still_active().await.is_ok());
    authority.transition("starting", Some("replacement")).await;
    assert!(authority.lock().await.is_err());
    authority.transition("running", Some("exact")).await;
    assert!(authority.still_active().await.is_err());
    authority.transition("starting", None).await;
    assert!(authority.lock().await.is_err());
    Ok(())
}
