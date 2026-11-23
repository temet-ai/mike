use std::path::Path;

use uuid::Uuid;

use super::ingest::{IngestError, ObserveSource};
use crate::runtime::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::workspace::runtime_launch::RuntimeLaunchIdentity;

async fn validate_launch(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    source: &ObserveSource,
    session: Uuid,
) -> Result<RuntimeLaunchIdentity, IngestError> {
    let home = source
        .codex_home
        .as_deref()
        .ok_or_else(|| IngestError::BadInput("spawn observation requires a Codex home".into()))?;
    let canonical = tokio::fs::canonicalize(home).await.map_err(|_| {
        IngestError::BadInput("spawn observation has an unavailable Codex home".into())
    })?;
    if crate::runtime::codex_home::namespace_for(&canonical).id != namespace.id {
        return Err(IngestError::BadInput(
            "spawn observation home is not verified".into(),
        ));
    }
    let launch = persist
        .runtime_launch_identity(session, RuntimeKind::Codex)
        .await?
        .ok_or_else(|| {
            IngestError::BadInput("no open Codex launch matches this spawn session".into())
        })?;
    if Path::new(&launch.runtime_home) != canonical {
        return Err(IngestError::BadInput(
            "spawn observation belongs to another Codex home".into(),
        ));
    }
    Ok(launch)
}

pub(super) async fn resolve(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    source: &ObserveSource,
    session: Uuid,
    thread: &str,
    cwd: Option<&str>,
) -> Result<Uuid, IngestError> {
    let provider = ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: thread.to_owned(),
    };
    provider
        .validate()
        .map_err(|error| IngestError::BadInput(error.to_string()))?;
    if let Some(binding) = persist.runtime().runtime_thread(&provider).await? {
        if binding.mike_session_id == session {
            return Ok(session);
        }
        return Err(IngestError::BadInput(
            "Codex thread is already bound to another Mike session".into(),
        ));
    }
    let launch = validate_launch(persist, namespace, source, session).await?;
    if cwd != Some(launch.folder.as_str()) {
        return Err(IngestError::BadInput(
            "first spawn observation must match the launch folder".into(),
        ));
    }
    persist
        .runtime()
        .register_runtime_namespace(namespace)
        .await?;
    persist.ensure_runtime_session(session).await?;
    persist
        .runtime()
        .bind_runtime_thread(&RuntimeThreadBinding {
            mike_session_id: session,
            provider,
            provider_session_id: Some(thread.to_owned()),
            owning_seat_id: None,
            native_parent: None,
            forked_from: None,
        })
        .await?;
    Ok(session)
}

pub(super) async fn telemetry_ready(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    source: &ObserveSource,
    session: Uuid,
) -> Result<bool, IngestError> {
    if let Some(binding) = persist.runtime().runtime_thread_by_session(session).await? {
        return if binding.provider.namespace_id == namespace.id {
            Ok(true)
        } else {
            Err(IngestError::BadInput(
                "spawn telemetry belongs to another namespace".into(),
            ))
        };
    }
    validate_launch(persist, namespace, source, session).await?;
    Ok(false)
}
