use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::context::{EnginePaths, EngineerContext, Machine, Program, upstream};
use super::sandbox::Binaries;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

pub(super) const BWRAP: &str = "/usr/bin/bwrap";
pub(super) const HOST: &str = "wright-test-host";

pub(super) fn bwrap_missing() -> Option<&'static str> {
    (!Path::new(BWRAP).is_file()).then_some(
        "skipped: /usr/bin/bwrap is not installed, so the real sandbox cannot be started",
    )
}

pub(super) fn credentials(path: &Path, valid_for: chrono::Duration) -> anyhow::Result<()> {
    let expires = (chrono::Utc::now() + valid_for).timestamp_millis();
    std::fs::write(
        path,
        serde_json::to_vec(&json!({
            "claudeAiOauth": {"accessToken": "test-access", "refreshToken": "test-refresh", "expiresAt": expires},
            "mcpOAuth": {"plugin:x|1": {"accessToken": "other-service-secret"}},
        }))?,
    )?;
    Ok(())
}

pub(super) fn paths(root: &Path) -> anyhow::Result<EnginePaths> {
    let slot = root.join("slot");
    std::fs::create_dir_all(slot.join("crates/sub"))?;
    std::fs::write(slot.join("README.md"), "slot")?;
    std::fs::write(
        slot.join(".env"),
        "DATABASE_URL=postgres://secret@127.0.0.1:5440/db",
    )?;
    std::fs::write(slot.join("crates/sub/.env.local"), "TOKEN=secret")?;
    std::fs::write(root.join("loadavg"), "0.00 0.00 0.00 1/100 1")?;
    credentials(&root.join("credentials.json"), chrono::Duration::days(1))?;
    let truth = PathBuf::from("/usr/bin/true");
    Ok(EnginePaths {
        repo: root.join("repo"),
        slot,
        refresh_slot: false,
        runs: root.join("runs"),
        credentials: root.join("credentials.json"),
        bwrap: PathBuf::from(BWRAP),
        binaries: Binaries {
            claude: truth.clone(),
            mike: truth.clone(),
            mike_mcp: truth,
        },
        loadavg: root.join("loadavg"),
        deploy_lock: root.join("deploy.lock"),
    })
}

pub(super) fn toggles(wright: bool) -> Arc<ArcSwap<FeatureToggles>> {
    Arc::new(ArcSwap::from_pointee(FeatureToggles {
        wright,
        ..FeatureToggles::default()
    }))
}

pub(super) struct Rig {
    pub(super) db: TestDbHandle,
    pub(super) ctx: EngineerContext,
    pub(super) tmp: tempfile::TempDir,
}

impl Rig {
    pub(super) async fn boot(program: Program) -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        let tmp = tempfile::tempdir()?;
        let ctx = EngineerContext {
            persist,
            toggles: toggles(true),
            shutdown: CancellationToken::new(),
            machine: Machine {
                host: HOST.to_owned(),
                uuid: None,
            },
            paths: paths(tmp.path())?,
            upstream: upstream("127.0.0.1:9", "127.0.0.1:9").map_err(anyhow::Error::msg)?,
            poll: Duration::from_millis(100),
            program,
        };
        Ok(Self { db, ctx, tmp })
    }

    pub(super) fn set_load(&self, load: &str) -> anyhow::Result<()> {
        std::fs::write(
            self.tmp.path().join("loadavg"),
            format!("{load} 0.00 0.00 1/100 1"),
        )?;
        Ok(())
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}

pub(super) fn script(body: &str) -> Program {
    Program::Script(vec![
        "/usr/bin/bash".to_owned(),
        "-c".to_owned(),
        body.to_owned(),
    ])
}

pub(super) fn processes_with(marker: &str) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .is_ok_and(|c| String::from_utf8_lossy(&c).contains(marker))
        })
        .collect()
}

pub(super) async fn until(what: &str, check: impl Fn() -> bool) -> anyhow::Result<()> {
    for _ in 0..200 {
        if check() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("{what} did not happen within 10 s")
}
