use std::path::{Path, PathBuf};

pub(super) type Eligible<'a> = &'a dyn Fn(&Path) -> bool;

pub(super) fn eligible_checkout(path: &Path, home: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let state_home = home.join(".local/state");
    if path
        .ancestors()
        .skip(1)
        .any(|ancestor| ancestor == state_home)
    {
        return false;
    }
    if format!("{}/", path.display()).contains("/tmp/") {
        return false;
    }
    path.is_dir() && path.join(".git").exists()
}

pub(super) fn local_checkout(
    recorded: &str,
    roots: &[String],
    eligible: Eligible<'_>,
) -> Option<PathBuf> {
    std::iter::once(recorded)
        .chain(roots.iter().map(String::as_str))
        .map(PathBuf::from)
        .find(|candidate| eligible(candidate))
}

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod checkout_tests;
