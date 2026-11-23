use std::path::PathBuf;

use uuid::Uuid;

use crate::runtime::{ProviderThreadRef, RuntimeKind, RuntimeNamespace, RuntimeThreadBinding};
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

pub struct ProxySource {
    pub launch: Option<Uuid>,
    pub home: Option<PathBuf>,
    pub thread: String,
    pub cwd: Option<String>,
}

pub struct ProxyIdentity {
    pub session_id: Uuid,
    pub cwd: Option<String>,
    pub codex_home: Option<PathBuf>,
}

pub async fn resolve(
    persist: &PersistHandle,
    source: &ProxySource,
) -> Result<ProxyIdentity, PersistError> {
    if source.thread.is_empty() || source.thread.len() > 512 || source.thread.contains('\0') {
        return Err(invalid("invalid Codex proxy thread identity"));
    }
    if let Some(launch) = source.launch {
        return spawned(persist, source, launch).await;
    }
    if source.home.is_none()
        && let Some(session_id) = persist
            .runtime()
            .resolve_unique_codex_session(&source.thread)
            .await?
    {
        return Ok(ProxyIdentity {
            session_id,
            cwd: source.cwd.clone(),
            codex_home: None,
        });
    }
    let namespace = namespace(source.home.as_deref());
    let provider = ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: source.thread.clone(),
    };
    let proposed = Uuid::new_v5(&namespace.id, source.thread.as_bytes());
    let session_id = bind(persist, &namespace, provider, proposed, None, false).await?;
    Ok(ProxyIdentity {
        session_id,
        cwd: source.cwd.clone(),
        codex_home: source.home.clone(),
    })
}

async fn spawned(
    persist: &PersistHandle,
    source: &ProxySource,
    launch_id: Uuid,
) -> Result<ProxyIdentity, PersistError> {
    let launch = persist
        .runtime_launch_identity(launch_id, RuntimeKind::Codex)
        .await?
        .ok_or_else(|| invalid("no open Codex launch matches the proxy route"))?;
    let home = PathBuf::from(launch.runtime_home);
    if source
        .home
        .as_ref()
        .is_some_and(|supplied| supplied != &home)
    {
        return Err(invalid("Codex proxy route belongs to another home"));
    }
    let namespace = namespace(Some(&home));
    let provider = ProviderThreadRef {
        namespace_id: namespace.id,
        thread_id: source.thread.clone(),
    };
    let root = persist
        .runtime()
        .runtime_thread_by_session(launch_id)
        .await?;
    let (proposed, parent) = match root {
        Some(root) if root.provider == provider => (launch_id, None),
        Some(root) if root.provider.namespace_id == namespace.id => (
            Uuid::new_v5(&namespace.id, source.thread.as_bytes()),
            Some((launch_id, root.provider)),
        ),
        Some(_) => return Err(invalid("Codex launch is bound to another namespace")),
        None => (launch_id, None),
    };
    let session_id = bind(persist, &namespace, provider, proposed, parent, true).await?;
    Ok(ProxyIdentity {
        session_id,
        cwd: Some(launch.folder),
        codex_home: Some(home),
    })
}

async fn bind(
    persist: &PersistHandle,
    namespace: &RuntimeNamespace,
    provider: ProviderThreadRef,
    proposed: Uuid,
    parent: Option<(Uuid, ProviderThreadRef)>,
    strict: bool,
) -> Result<Uuid, PersistError> {
    if let Some(binding) = persist.runtime().runtime_thread(&provider).await? {
        if strict
            && (binding.mike_session_id != proposed
                || binding.owning_seat_id != parent.as_ref().map(|(id, _)| *id))
        {
            return Err(invalid(
                "Codex thread already belongs to another Mike session",
            ));
        }
        return Ok(binding.mike_session_id);
    }
    persist
        .runtime()
        .register_runtime_namespace(namespace)
        .await?;
    persist.ensure_runtime_session(proposed).await?;
    persist
        .runtime()
        .bind_runtime_thread(&RuntimeThreadBinding {
            mike_session_id: proposed,
            provider_session_id: Some(provider.thread_id.clone()),
            provider,
            owning_seat_id: parent.as_ref().map(|(id, _)| *id),
            native_parent: parent.map(|(_, provider)| provider),
            forked_from: None,
        })
        .await?;
    Ok(proposed)
}

fn namespace(home: Option<&std::path::Path>) -> RuntimeNamespace {
    if let Some(home) = home {
        return super::codex_home::namespace_for(home);
    }
    let durable_key = "codex-proxy".to_string();
    RuntimeNamespace {
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, durable_key.as_bytes()),
        runtime: RuntimeKind::Codex,
        durable_key,
    }
}

fn invalid(message: &str) -> PersistError {
    PersistError::DataInvariant(message.into())
}

#[cfg(test)]
#[path = "codex_proxy_identity_tests.rs"]
mod tests;
