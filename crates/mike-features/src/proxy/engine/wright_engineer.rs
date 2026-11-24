use std::path::{Path, PathBuf};

use axum::http::HeaderMap;

pub const RUN_HEADER: &str = "x-mike-wright-run";

pub fn registration_path(headers: &HeaderMap, project_path: &str) -> String {
    if !project_path.is_empty() || !headers.contains_key(RUN_HEADER) {
        return project_path.to_owned();
    }
    let configured = std::env::var_os("MIKE_WRIGHT_REPO").map(PathBuf::from);
    mike_checkout(configured.or_else(|| std::env::current_dir().ok()))
        .map(|repo| repo.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn mike_checkout(candidate: Option<PathBuf>) -> Option<PathBuf> {
    candidate.filter(|repo| is_mike_checkout(repo))
}

fn is_mike_checkout(repo: &Path) -> bool {
    repo.join("crates/mike/Cargo.toml").is_file()
}

#[cfg(test)]
#[path = "wright_engineer_tests.rs"]
mod tests;
