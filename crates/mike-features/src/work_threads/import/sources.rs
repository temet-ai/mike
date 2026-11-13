use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::store::persistence::PersistHandle;

const LAYOUTS: [&str; 2] = ["artifacts/threads", "docs/threads"];

pub struct WorklogFile {
    pub slug: String,
    pub path: PathBuf,
}

pub struct ResolvedProject {
    pub id: i64,
    pub name: String,
    pub roots: Vec<String>,
}

pub fn find_worklogs(repo: &Path) -> Vec<WorklogFile> {
    let mut found: Vec<WorklogFile> = Vec::new();
    for checkout in checkouts(repo) {
        for file in worklogs_in_checkout(&checkout) {
            keep_newest(&mut found, file);
        }
    }
    found
}

fn checkouts(repo: &Path) -> Vec<PathBuf> {
    let mut list = vec![repo.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(repo.join(".claude/worktrees")) {
        let mut worktrees: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|path| path.is_dir())
            .collect();
        worktrees.sort();
        list.extend(worktrees);
    }
    list
}

fn worklogs_in_checkout(checkout: &Path) -> Vec<WorklogFile> {
    let mut found: Vec<WorklogFile> = Vec::new();
    for layout in LAYOUTS {
        let Ok(entries) = std::fs::read_dir(checkout.join(layout)) else {
            continue;
        };
        let mut directories: Vec<PathBuf> =
            entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        directories.sort();
        for directory in directories {
            let path = directory.join("worklog.md");
            let slug = directory
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned);
            if let Some(slug) = slug
                && path.is_file()
                && !found.iter().any(|file| file.slug == slug)
            {
                found.push(WorklogFile { slug, path });
            }
        }
    }
    found
}

fn keep_newest(found: &mut Vec<WorklogFile>, file: WorklogFile) {
    match found.iter_mut().find(|known| known.slug == file.slug) {
        Some(known) if modified(&file.path) > modified(&known.path) => *known = file,
        Some(_) => {}
        None => found.push(file),
    }
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn candidates(repo: &str) -> Vec<String> {
    let mut list = vec![repo.to_owned()];
    if let Some((main, _)) = repo.split_once("/.claude/worktrees/") {
        list.push(main.to_owned());
    }
    let common = Command::new("git")
        .args([
            "-C",
            repo,
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    if let Some(common) = common
        && let Some(main) = common.trim().strip_suffix("/.git")
    {
        list.push(main.to_owned());
    }
    list.dedup();
    list
}

pub async fn resolve_repo(
    handle: &PersistHandle,
    repo: &str,
) -> Result<ResolvedProject, String> {
    for candidate in candidates(repo) {
        match handle
            .launch()
            .resolve_work_thread_project(&candidate)
            .await
        {
            Ok(Some(project)) => return finish(handle, repo, project).await,
            Ok(None) => {}
            Err(other) => return Err(other.to_string()),
        }
    }
    Err("no mike project has this folder, or the checkout it belongs to, as a location; register it with register_project_location".to_owned())
}

async fn finish(
    handle: &PersistHandle,
    repo: &str,
    project: crate::store::persistence::project_knowledge::ProjectIdentity,
) -> Result<ResolvedProject, String> {
    let mut roots = handle
        .launch()
        .work_thread_project_roots(project.id)
        .await
        .map_err(|error| error.to_string())?;
    roots.push(repo.to_owned());
    Ok(ResolvedProject {
        id: project.id,
        name: project.project_path,
        roots,
    })
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
