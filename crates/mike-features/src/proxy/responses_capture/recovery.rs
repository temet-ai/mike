use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use super::ResponsesState;
use super::spool::{Manifest, lock, write_manifest};

pub(super) struct Schedule {
    started: Instant,
    running: AtomicBool,
    next_attempt: AtomicU64,
    failures: AtomicU32,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            running: AtomicBool::new(false),
            next_attempt: AtomicU64::new(0),
            failures: AtomicU32::new(0),
        }
    }
}

struct Running(Arc<Schedule>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
    }
}

pub(super) fn maybe_start(state: &ResponsesState) {
    let schedule = &state.recovery;
    let now = schedule.started.elapsed().as_secs();
    if now < schedule.next_attempt.load(Ordering::Acquire)
        || schedule.running.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let running = Running(schedule.clone());
    let worker = state.clone();
    state.tasks.spawn(async move {
        let succeeded = recover_all(worker).await;
        let schedule = &running.0;
        let failures = if succeeded {
            schedule.failures.store(0, Ordering::Release);
            0
        } else {
            schedule
                .failures
                .fetch_add(1, Ordering::AcqRel)
                .saturating_add(1)
                .min(5)
        };
        let delay = 30_u64.saturating_mul(1_u64 << failures).min(600);
        schedule.next_attempt.store(
            schedule.started.elapsed().as_secs() + delay,
            Ordering::Release,
        );
    });
}

#[cfg(test)]
pub(super) async fn recover(state: ResponsesState) {
    let _ = recover_all(state).await;
}

async fn recover_all(state: ResponsesState) -> bool {
    let Some(persist) = &state.persist else {
        return true;
    };
    let Some(root) = state.capture_root.as_deref() else {
        return false;
    };
    match recover_directory(&state, persist, root).await {
        Ok(succeeded) => succeeded,
        Err(error) => {
            tracing::error!(error, "Codex capture staging scan failed");
            false
        }
    }
}

async fn recover_directory(
    state: &ResponsesState,
    persist: &crate::store::persistence::PersistHandle,
    root: &std::path::Path,
) -> Result<bool, String> {
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.to_string()),
    };
    let mut succeeded = true;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        succeeded &= recover_entry(state, persist, entry).await;
    }
    Ok(succeeded)
}

async fn recover_entry(
    state: &ResponsesState,
    persist: &crate::store::persistence::PersistHandle,
    entry: tokio::fs::DirEntry,
) -> bool {
    if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
        return true;
    }
    let path = entry.path();
    match recover_one(state, persist, &path).await {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(error, staging = %path.display(), "Codex capture recovery failed; files retained");
            false
        }
    }
}

async fn recover_one(
    state: &ResponsesState,
    persist: &crate::store::persistence::PersistHandle,
    path: &std::path::Path,
) -> Result<(), String> {
    let Some(_lock) = lock(path).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let bytes = match tokio::fs::read(path.join("manifest.json")).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return remove_if_only_lock(path).await;
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if manifest.exchange.status == "pending" {
        manifest.exchange.status = "aborted".into();
        manifest.exchange.metadata["error"] =
            serde_json::json!("capture recovered after the proxy stopped before finalization");
        manifest.response_complete = false;
        manifest.response_received |= tokio::fs::metadata(path.join("response"))
            .await
            .is_ok_and(|file| file.len() > 0);
        write_manifest(path, &manifest).await?;
    }
    recover_identity(state, persist, &mut manifest).await?;
    write_manifest(path, &manifest).await?;
    super::capture_write::persist(persist, path, &mut manifest).await?;
    super::record::finish_delivery(persist, &manifest)
        .await
        .map_err(|error| error.to_string())?;
    tokio::fs::remove_dir_all(path)
        .await
        .map_err(|error| error.to_string())
}

async fn remove_if_only_lock(path: &std::path::Path) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(path)
        .await
        .map_err(|error| error.to_string())?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        if entry.file_name() != "capture.lock" {
            return Err(format!(
                "no manifest.json, but {} is staged",
                entry.file_name().to_string_lossy()
            ));
        }
    }
    tokio::fs::remove_dir_all(path)
        .await
        .map_err(|error| error.to_string())
}

async fn recover_identity(
    state: &ResponsesState,
    persist: &crate::store::persistence::PersistHandle,
    manifest: &mut Manifest,
) -> Result<(), String> {
    if manifest.exchange.session_uuid.is_some() {
        super::record::register_notification_target(
            persist,
            &manifest.identity,
            manifest.exchange.session_uuid,
        )
        .await;
        return Ok(());
    }
    let Some(identity) =
        super::wire_observation::resolve_source(persist, &manifest.identity).await?
    else {
        return Ok(());
    };
    manifest.exchange.session_uuid = Some(identity.session_id);
    manifest.identity.home = identity.codex_home;
    manifest.exchange.metadata["correlation"]["normalized_session"] =
        serde_json::json!({"source":"recovered_proxy_identity", "session_id":identity.session_id});
    let correlation = super::Correlation {
        session_uuid: Some(identity.session_id),
        cwd: identity.cwd,
        ..Default::default()
    };
    super::record::register_session(state, persist, &correlation).await;
    super::record::register_notification_target(
        persist,
        &manifest.identity,
        Some(identity.session_id),
    )
    .await;
    Ok(())
}
