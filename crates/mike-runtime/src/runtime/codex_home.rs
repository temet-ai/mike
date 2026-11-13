use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::runtime::{RuntimeKind, RuntimeNamespace};
use crate::store::persistence::PersistHandle;

pub fn namespace_for(codex_home: &Path) -> RuntimeNamespace {
    let durable_key = format!("codex-home:{}", codex_home.display());
    RuntimeNamespace {
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, durable_key.as_bytes()),
        runtime: RuntimeKind::Codex,
        durable_key,
    }
}

fn operator_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
}

pub async fn verified_home(candidate: &str) -> Option<PathBuf> {
    let expected = tokio::fs::canonicalize(operator_home()?).await.ok()?;
    let candidate = tokio::fs::canonicalize(candidate).await.ok()?;
    (candidate == expected).then_some(expected)
}

pub async fn observe_local_codex(
    persist: &PersistHandle,
    session: Uuid,
    thread: &str,
    home: Option<&str>,
) {
    let Some(home) = home else { return };
    let Some(home) = verified_home(home).await else {
        return;
    };
    if Uuid::parse_str(thread).is_err() {
        return;
    }
    if let Err(error) = persist
        .runtime()
        .register_runtime_notification_target(session, thread, &home.to_string_lossy())
        .await
    {
        tracing::warn!(session_id = %session, error = %error, "Codex local mail notification target not recorded");
    }
}
