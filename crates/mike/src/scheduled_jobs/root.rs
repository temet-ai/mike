use std::path::{Path, PathBuf};

use super::jobs::Job;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    Present,
    Missing,
    NeedsCheckout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Located {
    pub(crate) root: Option<PathBuf>,
    pub(crate) presence: Presence,
}

impl Located {
    pub(crate) fn runs_here(&self) -> bool {
        self.root.is_some() && self.presence != Presence::NeedsCheckout
    }
}

fn has_script(root: &Path, job: &Job) -> bool {
    root.join(job.script).is_file()
}

fn checkout_candidates(exe: Option<&Path>, cwd: Option<&Path>) -> Vec<PathBuf> {
    let from_cwd = cwd.into_iter().flat_map(Path::ancestors);
    let real_exe = exe.and_then(|exe| std::fs::canonicalize(exe).ok());
    let from_exe = real_exe.iter().flat_map(|exe| exe.ancestors());
    from_cwd.chain(from_exe).map(Path::to_path_buf).collect()
}

pub(crate) fn locate_in(job: &Job, exe: Option<&Path>, cwd: Option<&Path>) -> Located {
    let checkout = checkout_candidates(exe, cwd)
        .into_iter()
        .find(|dir| has_script(dir, job));
    if let Some(root) = checkout {
        return found(root);
    }
    if job.checkout_only {
        return Located {
            root: cwd.map(Path::to_path_buf),
            presence: Presence::NeedsCheckout,
        };
    }
    let package = exe
        .into_iter()
        .flat_map(crate::claude_code::mods::package_dirs)
        .find(|share| has_script(share, job));
    match package {
        Some(root) => found(root),
        None => Located {
            root: cwd.map(Path::to_path_buf),
            presence: Presence::Missing,
        },
    }
}

fn found(root: PathBuf) -> Located {
    Located {
        root: Some(root),
        presence: Presence::Present,
    }
}

pub(crate) fn locate(job: &Job) -> Located {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    locate_in(job, exe.as_deref(), cwd.as_deref())
}

#[cfg(test)]
#[path = "root_tests.rs"]
mod root_tests;
