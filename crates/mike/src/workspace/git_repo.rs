use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoIdentity {
    pub(crate) checkout_root: PathBuf,
    pub(crate) main_root: PathBuf,
    pub(crate) origin_url: Option<String>,
}

pub(crate) fn repo_identity(cwd: &Path) -> Option<RepoIdentity> {
    let checkout_root = cwd.ancestors().find(|dir| {
        let dot_git = dir.join(".git");
        dot_git.is_file() || valid_git_directory(&dot_git, &dot_git)
    })?;
    let dot_git = checkout_root.join(".git");
    let common_dir = if dot_git.is_dir() {
        dot_git
    } else {
        common_dir_of_worktree(&dot_git)?
    };
    let main_root = main_root_of(&common_dir)?;
    Some(RepoIdentity {
        checkout_root: checkout_root.to_path_buf(),
        main_root,
        origin_url: origin_url(&common_dir),
    })
}

fn common_dir_of_worktree(dot_git_file: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(dot_git_file).ok()?;
    let gitdir = contents
        .lines()
        .find_map(|l| l.strip_prefix("gitdir:"))?
        .trim();
    let base = dot_git_file.parent()?;
    let gitdir = base.join(gitdir);
    let common = match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(rel) => gitdir.join(rel.trim()),
        Err(_) => gitdir.clone(),
    };
    let common = std::fs::canonicalize(common).ok()?;
    valid_git_directory(&gitdir, &common).then_some(common)
}

fn valid_git_directory(git_dir: &Path, common_dir: &Path) -> bool {
    valid_head(&git_dir.join("HEAD"))
        && common_dir.join("objects").is_dir()
        && common_dir.join("refs").is_dir()
}

fn valid_head(path: &Path) -> bool {
    if let Ok(reference) = std::fs::read_link(path) {
        return reference.starts_with("refs");
    }
    let Ok(head) = std::fs::read_to_string(path) else {
        return false;
    };
    let head = head.trim();
    head.strip_prefix("ref:")
        .is_some_and(|reference| reference.trim_start().starts_with("refs/"))
        || (matches!(head.len(), 40 | 64) && head.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn main_root_of(common_dir: &Path) -> Option<PathBuf> {
    if common_dir.file_name()? == ".git" {
        return common_dir.parent().map(Path::to_path_buf);
    }
    Some(common_dir.to_path_buf())
}

fn origin_url(common_dir: &Path) -> Option<String> {
    let config = std::fs::read_to_string(common_dir.join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "url"
        {
            return Some(normalize_remote(value.trim()));
        }
    }
    None
}

pub(crate) fn normalize_remote(url: &str) -> String {
    let url = url.trim_end_matches('/').trim_end_matches(".git");
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let without_user = without_scheme
        .split_once('@')
        .map_or(without_scheme, |(_, rest)| rest);
    without_user.replacen(':', "/", 1).to_ascii_lowercase()
}

#[cfg(test)]
#[path = "git_repo_tests.rs"]
mod tests;
