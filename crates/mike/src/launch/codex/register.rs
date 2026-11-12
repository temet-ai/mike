use std::path::Path;

use uuid::Uuid;

use crate::runtime::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};
use crate::store::persistence::PersistHandle;

pub const CODEX_BACKEND: &str = "none";
pub const CODEX_BACKEND_REASON: &str =
    "codex seat: controlled through `codex app-server`, no terminal surface";

pub fn namespace_for(codex_home: &Path) -> RuntimeNamespace {
    let durable_key = format!("codex-home:{}", codex_home.display());
    RuntimeNamespace {
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, durable_key.as_bytes()),
        runtime: RuntimeKind::Codex,
        durable_key,
    }
}

pub async fn bind_seat_thread(
    persist: &PersistHandle,
    codex_home: &Path,
    session: Uuid,
    thread_id: &str,
    forked_from: Option<&str>,
) -> anyhow::Result<()> {
    let record = |error: crate::store::persistence::PersistError| {
        anyhow::anyhow!(
            "could not record Codex thread {thread_id} for seat session {session}: {error}"
        )
    };
    let namespace = namespace_for(codex_home);
    persist
        .register_runtime_namespace(&namespace)
        .await
        .map_err(record)?;
    persist
        .ensure_runtime_session(session)
        .await
        .map_err(record)?;
    let provider = |thread: &str| ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: thread.to_string(),
    };
    let binding = RuntimeThreadBinding {
        mike_session_id: session,
        provider: provider(thread_id),
        provider_session_id: Some(thread_id.to_string()),
        owning_seat_id: None,
        native_parent: None,
        forked_from: forked_from.map(provider),
    };
    persist
        .bind_runtime_thread(&binding)
        .await
        .map(|_| ())
        .map_err(record)
}

pub(crate) async fn bound_thread(
    persist: &PersistHandle,
    codex_home: &Path,
    session: Uuid,
) -> anyhow::Result<Option<String>> {
    let namespace = namespace_for(codex_home);
    let binding = persist
        .runtime_thread_by_session(session)
        .await
        .map_err(|error| {
            anyhow::anyhow!("could not read the Codex thread of seat session {session}: {error}")
        })?;
    Ok(binding
        .filter(|b| b.provider.namespace_id == namespace.id)
        .map(|b| b.provider.thread_id))
}
