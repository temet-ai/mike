use uuid::Uuid;

use super::ingest::IngestError;
use crate::runtime::{ProviderThreadRef, RuntimeNamespace, RuntimeThreadBinding};
use crate::store::persistence::workspace::RegisterWorkspaceSession;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn resolve(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    thread: &str,
) -> Result<Uuid, IngestError> {
    resolve_with_parent(persist, namespace, thread, None).await
}

pub(super) async fn resolve_child(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    parent: &str,
    thread: &str,
) -> Result<Uuid, IngestError> {
    let owner = resolve(persist, namespace, parent).await?;
    resolve_with_parent(persist, namespace, thread, Some((owner, parent))).await
}

async fn resolve_with_parent(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    thread: &str,
    parent: Option<(Uuid, &str)>,
) -> Result<Uuid, IngestError> {
    if thread.is_empty() || thread.len() > 512 || thread.contains('\0') {
        return Err(IngestError::BadInput("invalid provider thread id".into()));
    }
    persist.register_runtime_namespace(namespace).await?;
    let provider = ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: thread.to_owned(),
    };
    if let Some(binding) = persist.runtime_thread(&provider).await? {
        return Ok(binding.mike_session_id);
    }
    let mut session =
        Uuid::parse_str(thread).unwrap_or_else(|_| Uuid::new_v5(&namespace.id, thread.as_bytes()));
    if let Some(binding) = persist.runtime_thread_by_session(session).await? {
        if binding.provider == provider {
            return Ok(binding.mike_session_id);
        }
        session = Uuid::new_v5(&namespace.id, thread.as_bytes());
    }
    persist.ensure_runtime_session(session).await?;
    persist
        .bind_runtime_thread(&RuntimeThreadBinding {
            mike_session_id: session,
            provider,
            provider_session_id: Some(thread.to_owned()),
            owning_seat_id: parent.map(|(owner, _)| owner),
            native_parent: parent.map(|(_, thread)| ProviderThreadRef {
                namespace_id: namespace.id,
                thread_id: thread.to_owned(),
            }),
            forked_from: None,
        })
        .await?;
    Ok(session)
}

pub(super) async fn register(
    persist: &PersistHandle,
    session: Uuid,
    cwd: Option<&str>,
) -> Result<(), IngestError> {
    if cwd.is_some_and(|cwd| cwd.len() > 4096 || cwd.contains('\0')) {
        return Err(IngestError::BadInput(
            "invalid Codex working directory".into(),
        ));
    }
    match persist.renew_workspace_lease(session).await {
        Ok(_) => {}
        Err(PersistError::RegistrationNotFound(_)) => {
            if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
                persist
                    .register_runtime_workspace_session(RegisterWorkspaceSession {
                        session_uuid: session,
                        project_path: Some(cwd),
                        cwd,
                        worktree: None,
                        registered_via: "hook",
                        intent: None,
                        role: None,
                        name: None,
                        instructions: None,
                        pid: None,
                    })
                    .await?;
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(super) async fn end(persist: &PersistHandle, session: Uuid) -> Result<(), IngestError> {
    match persist.end_workspace_registration(session, None).await {
        Ok(_) | Err(PersistError::RegistrationNotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
