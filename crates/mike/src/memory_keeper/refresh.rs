use std::collections::HashMap;
use std::sync::Arc;

use super::resolve::ThreadFacts;
use super::snapshot::{KeeperCache, KeeperSnapshot, ProjectThreads, ThreadEntry};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::work_threads::WorkThreadError;
use crate::store::persistence::work_threads::cards::{LocationRoot, OpenCardRow};

pub(crate) async fn refresh(
    persist: &PersistHandle,
    cache: &KeeperCache,
) -> Result<bool, WorkThreadError> {
    let generation = persist.thread_cards_generation().await?;
    if cache.load().generation == generation && !cache.load().roots.is_empty() {
        return Ok(false);
    }
    let (rows, roots) = tokio::try_join!(persist.open_thread_cards(), persist.location_roots())?;
    cache.store(Arc::new(build(generation, rows, roots)));
    Ok(true)
}

pub(crate) fn build(
    generation: i64,
    rows: Vec<OpenCardRow>,
    roots: Vec<LocationRoot>,
) -> KeeperSnapshot {
    let mut projects: HashMap<i64, ProjectThreads> = HashMap::new();
    for row in rows {
        projects
            .entry(row.project_id)
            .or_default()
            .threads
            .push(entry(row));
    }
    KeeperSnapshot {
        generation,
        projects: projects
            .into_iter()
            .map(|(id, threads)| (id, Arc::new(threads)))
            .collect(),
        roots: roots
            .into_iter()
            .map(|root| (root.root_path, root.project_id))
            .collect(),
    }
}

fn entry(row: OpenCardRow) -> ThreadEntry {
    ThreadEntry {
        thread_id: row.thread_id,
        facts: ThreadFacts {
            slug: row.slug,
            title: row.title,
            goal: row.goal,
            status: row.status,
            branch: row.branch,
            worktree: row.worktree,
            last_session_uuid: row.last_session_uuid,
            last_checkpoint_at: row.last_checkpoint_at,
            linked_sessions: row.linked_sessions.into_iter().collect(),
            checkout_dirs: row.checkout_dirs.into_iter().collect(),
        },
        text: Arc::from(row.text.as_str()),
        content_hash: row.content_hash,
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
