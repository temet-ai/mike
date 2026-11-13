use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::change_record::watch::FileDelta;
use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewTreeChange;
use follow::{FleetTree, follow_feed};

const POLL: Duration = Duration::from_secs(1);
const STATUS: [&str; 4] = ["status", "--porcelain=v2", "-z", "--untracked-files=all"];
const ADDED: &str = "added";
const MODIFIED: &str = "modified";
const DELETED: &str = "deleted";

fn watchers() -> &'static DashMap<String, CancellationToken> {
    static WATCHERS: OnceLock<DashMap<String, CancellationToken>> = OnceLock::new();
    WATCHERS.get_or_init(DashMap::new)
}

pub fn ensure(state: &AppState, run_id: &str, repo: &str) {
    if watchers().contains_key(run_id) {
        return;
    }
    let token = state.shutdown().child_token();
    watchers().insert(run_id.to_string(), token.clone());
    let (state, run, repo) = (state.clone(), run_id.to_string(), repo.to_string());
    state.tracker.clone().spawn(async move {
        if let Some(persist) = state.persist().cloned() {
            let toggles = state.feature_toggle_overlay.clone();
            watch(&persist, &toggles, &run, &repo, token).await;
        }
        watchers().remove(&run);
    });
}

pub fn stop(run_id: &str) {
    if let Some((_, token)) = watchers().remove(run_id) {
        token.cancel();
    }
}

pub fn watched() -> Vec<String> {
    watchers().iter().map(|entry| entry.key().clone()).collect()
}

pub fn is_watching(run_id: &str) -> bool {
    watchers().contains_key(run_id)
}

async fn watch(
    persist: &PersistHandle,
    toggles: &Toggles,
    run: &str,
    repo: &str,
    token: CancellationToken,
) {
    let Some(mut tree) = Tree::baseline(repo).await else {
        note_unreadable(run, repo);
        return;
    };
    note_watching(run, repo, tree.known.len());
    let fleet = FleetTree {
        persist,
        toggles,
        run,
        repo,
    };
    loop {
        if let Some(feed) = feed_for(toggles, repo) {
            follow_feed(&fleet, &mut tree, feed, &token).await;
        }
        if !wait_for_tick(&token).await {
            return;
        }
        record(persist, run, &tree.observe(repo).await).await;
    }
}

type Toggles = Arc<ArcSwap<FeatureToggles>>;

fn feed_for(toggles: &Toggles, repo: &str) -> Option<broadcast::Receiver<Vec<FileDelta>>> {
    if !toggles.load().change_record {
        return None;
    }
    crate::change_record::watch::subscribe(repo)
}

fn note_unreadable(run: &str, repo: &str) {
    tracing::warn!(%run, %repo, "fleet: could not read the checkout, so its changes are not recorded");
}

fn note_watching(run: &str, repo: &str, files: usize) {
    tracing::info!(%run, %repo, files, "fleet: watching the checkout for changes");
}

async fn wait_for_tick(token: &CancellationToken) -> bool {
    tokio::select! {
        () = token.cancelled() => false,
        () = tokio::time::sleep(POLL) => true,
    }
}

async fn record(persist: &PersistHandle, run: &str, changes: &[NewTreeChange]) {
    if changes.is_empty() {
        return;
    }
    if let Err(err) = persist.fleet().insert_tree_changes(run, changes).await {
        tracing::warn!(%run, error = %err, "fleet: could not record changes in the checkout");
    }
}

#[derive(Debug, Default)]
pub struct Tree {
    pub known: BTreeMap<String, String>,
    last_status: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    Present(String),
    Gone(String),
}

impl Tree {
    pub async fn baseline(repo: &str) -> Option<Self> {
        let status = git(repo, &STATUS, None).await?;
        let known = current(repo, &status).await?;
        Some(Self {
            known,
            last_status: status,
        })
    }

    pub fn apply_feed(&mut self, deltas: &[FileDelta]) -> Vec<NewTreeChange> {
        self.forget_status();
        let mut out = Vec::with_capacity(deltas.len());
        for delta in deltas {
            match &delta.after_sha {
                Some(sha) => {
                    self.known.insert(delta.path.clone(), sha.clone());
                }
                None => {
                    self.known.remove(&delta.path);
                }
            }
            out.push(change(
                &delta.path,
                delta.change.as_str(),
                delta.after_sha.as_ref(),
            ));
        }
        out
    }

    pub fn forget_status(&mut self) {
        self.last_status.clear();
    }

    pub async fn observe(&mut self, repo: &str) -> Vec<NewTreeChange> {
        let Some(status) = git(repo, &STATUS, None).await else {
            return Vec::new();
        };
        if status == self.last_status {
            return Vec::new();
        }
        let Some(now) = current(repo, &status).await else {
            return Vec::new();
        };
        self.last_status = status;
        let changes = differences(&self.known, &now);
        self.known = now;
        changes
    }
}

async fn current(repo: &str, status: &[u8]) -> Option<BTreeMap<String, String>> {
    let index = git(repo, &["ls-files", "-s", "-z"], None).await?;
    let mut now = parse_index(&index);
    apply_hashes(repo, &mut now, &parse_status(status)).await;
    Some(now)
}

pub fn differences(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<NewTreeChange> {
    let mut out = Vec::new();
    for (path, sha) in after {
        match before.get(path) {
            None => out.push(change(path, ADDED, Some(sha))),
            Some(old) if old != sha => out.push(change(path, MODIFIED, Some(sha))),
            Some(_) => {}
        }
    }
    for path in before.keys().filter(|p| !after.contains_key(*p)) {
        out.push(change(path, DELETED, None));
    }
    out
}

fn change(path: &str, kind: &str, sha: Option<&String>) -> NewTreeChange {
    NewTreeChange {
        path: path.to_string(),
        change: kind.to_string(),
        content_sha: sha.cloned(),
    }
}

pub fn parse_index(raw: &[u8]) -> BTreeMap<String, String> {
    let mut known = BTreeMap::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let Some((meta, path)) = text.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (Some(_mode), Some(sha), Some(stage)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if stage == "0" {
            known.insert(path.to_string(), sha.to_string());
        }
    }
    known
}

pub fn parse_status(raw: &[u8]) -> Vec<Seen> {
    let mut out = Vec::new();
    let mut entries = raw.split(|b| *b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let text = String::from_utf8_lossy(entry);
        match text.as_bytes().first() {
            Some(b'1') => out.extend(changed_entry(&text, 8)),
            Some(b'2') => {
                out.extend(changed_entry(&text, 9));
                entries.next();
            }
            Some(b'?') => out.push(Seen::Present(text[2..].to_string())),
            Some(b'u') => out.extend(changed_entry(&text, 10)),
            _ => {}
        }
    }
    out
}

fn changed_entry(text: &str, path_field: usize) -> Option<Seen> {
    let fields: Vec<&str> = text.splitn(path_field + 1, ' ').collect();
    let path = fields.get(path_field)?.to_string();
    let worktree = fields.get(1)?.chars().nth(1)?;
    Some(if worktree == 'D' {
        Seen::Gone(path)
    } else {
        Seen::Present(path)
    })
}

async fn apply_hashes(repo: &str, known: &mut BTreeMap<String, String>, seen: &[Seen]) {
    let present: Vec<&str> = seen
        .iter()
        .filter_map(|s| match s {
            Seen::Present(p) => Some(p.as_str()),
            Seen::Gone(_) => None,
        })
        .collect();
    for path in seen.iter().filter_map(|s| match s {
        Seen::Gone(p) => Some(p),
        Seen::Present(_) => None,
    }) {
        known.remove(path);
    }
    if present.is_empty() {
        return;
    }
    let input = present.join("\n") + "\n";
    let Some(out) = git(
        repo,
        &["hash-object", "--stdin-paths"],
        Some(input.as_bytes()),
    )
    .await
    else {
        return;
    };
    let shas = String::from_utf8_lossy(&out);
    let mut gone: BTreeSet<&str> = BTreeSet::new();
    for (path, sha) in present.iter().zip(shas.lines()) {
        if sha.is_empty() {
            gone.insert(path);
        } else {
            known.insert((*path).to_string(), sha.to_string());
        }
    }
    for path in gone {
        known.remove(path);
    }
}

pub(super) async fn git(repo: &str, args: &[&str], stdin: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut command = tokio::process::Command::new("git");
    command.arg("-C").arg(repo).args(args);
    command.stdin(if stdin.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    });
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::null());
    let mut child = command.spawn().ok()?;
    let pipe = child.stdin.take();
    let feed = async move {
        if let (Some(input), Some(mut pipe)) = (stdin, pipe) {
            pipe.write_all(input).await.ok()?;
        }
        Some(())
    };
    let (fed, output) = tokio::join!(feed, child.wait_with_output());
    let output = output.ok()?;
    (fed.is_some() && output.status.success()).then_some(output.stdout)
}

mod follow;

#[cfg(test)]
mod feed_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_listing_gives_a_hash_per_tracked_path() {
        let raw =
            b"100644 aaaa 0\tsrc/lib.rs\x00100644 bbbb 0\tREADME.md\x00100644 cccc 1\tconflict.rs\x00";
        let known = parse_index(raw);
        assert_eq!(known.get("src/lib.rs").map(String::as_str), Some("aaaa"));
        assert_eq!(known.get("README.md").map(String::as_str), Some("bbbb"));
        assert!(
            !known.contains_key("conflict.rs"),
            "an unmerged entry is not a known state"
        );
    }

    #[test]
    fn the_status_listing_tells_present_from_gone() {
        let raw = b"1 .M N... 100644 100644 100644 aaaa aaaa src/lib.rs\0\
                    1 .D N... 100644 100644 000000 bbbb bbbb gone.rs\0\
                    2 R. N... 100644 100644 100644 cccc cccc R100 new.rs\0old.rs\0\
                    ? notes.md\0";
        let seen = parse_status(raw);
        assert_eq!(
            seen,
            vec![
                Seen::Present("src/lib.rs".into()),
                Seen::Gone("gone.rs".into()),
                Seen::Present("new.rs".into()),
                Seen::Present("notes.md".into()),
            ]
        );
    }

    #[test]
    fn differences_name_what_was_added_changed_and_removed() {
        let before: BTreeMap<String, String> = [("a.rs", "1"), ("b.rs", "2"), ("c.rs", "3")]
            .into_iter()
            .map(|(p, s)| (p.into(), s.into()))
            .collect();
        let after: BTreeMap<String, String> = [("a.rs", "1"), ("b.rs", "9"), ("d.rs", "4")]
            .into_iter()
            .map(|(p, s)| (p.into(), s.into()))
            .collect();
        let changes = differences(&before, &after);
        let summary: Vec<(String, String)> = changes
            .iter()
            .map(|c| (c.path.clone(), c.change.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("b.rs".into(), "modified".into()),
                ("d.rs".into(), "added".into()),
                ("c.rs".into(), "deleted".into())
            ]
        );
        assert_eq!(changes[2].content_sha, None);
    }

    #[tokio::test]
    async fn a_real_checkout_reports_a_write_once_and_then_nothing() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().to_string_lossy().into_owned();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "t"],
        ] {
            git(&repo, &args, None)
                .await
                .ok_or_else(|| anyhow::anyhow!("git {args:?}"))?;
        }
        std::fs::write(dir.path().join("a.txt"), "one")?;
        git(&repo, &["add", "a.txt"], None).await;
        git(&repo, &["commit", "-q", "-m", "base"], None).await;
        let mut tree = Tree::baseline(&repo)
            .await
            .ok_or_else(|| anyhow::anyhow!("baseline"))?;
        assert!(tree.observe(&repo).await.is_empty());
        std::fs::write(dir.path().join("a.txt"), "two")?;
        std::fs::write(dir.path().join("b.txt"), "new")?;
        let changes = tree.observe(&repo).await;
        let summary: Vec<(String, String)> = changes
            .iter()
            .map(|c| (c.path.clone(), c.change.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("a.txt".into(), "modified".into()),
                ("b.txt".into(), "added".into())
            ]
        );
        assert!(
            tree.observe(&repo).await.is_empty(),
            "nothing changed since"
        );
        std::fs::remove_file(dir.path().join("b.txt"))?;
        let gone = tree.observe(&repo).await;
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].change, "deleted");
        Ok(())
    }

    #[tokio::test]
    async fn a_tree_of_two_thousand_changed_paths_is_hashed_without_stalling() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().to_string_lossy().into_owned();
        git(&repo, &["init", "-q", "-b", "main"], None)
            .await
            .ok_or_else(|| anyhow::anyhow!("git init"))?;
        for n in 0..2000 {
            std::fs::write(dir.path().join(format!("file-{n:04}.txt")), format!("{n}"))?;
        }
        let baseline = tokio::time::timeout(Duration::from_secs(60), Tree::baseline(&repo))
            .await
            .map_err(|_| anyhow::anyhow!("hashing 2,000 changed paths stalled"))?
            .ok_or_else(|| anyhow::anyhow!("baseline"))?;
        assert_eq!(baseline.known.len(), 2000);
        Ok(())
    }
}
