use std::collections::HashSet;

use super::diff_git::{run, valid_object};

pub(super) async fn on_a_remote(repo: &str, shas: &[String]) -> HashSet<String> {
    let asked: Vec<&str> = shas
        .iter()
        .map(String::as_str)
        .filter(|s| valid_object(s))
        .collect();
    if asked.is_empty() {
        return HashSet::new();
    }
    let Some(present) = rev_list(repo, &["--no-walk=unsorted"], &asked, &[]).await else {
        return HashSet::new();
    };
    let present: Vec<&str> = present.iter().map(String::as_str).collect();
    let Some(local) = rev_list(repo, &[], &present, &["--not", "--remotes"]).await else {
        return HashSet::new();
    };
    present
        .into_iter()
        .filter(|sha| !local.contains(*sha))
        .map(str::to_owned)
        .collect()
}

async fn rev_list(
    repo: &str,
    before: &[&str],
    shas: &[&str],
    after: &[&str],
) -> Option<HashSet<String>> {
    let mut args = vec!["rev-list", "--ignore-missing"];
    args.extend_from_slice(before);
    args.extend_from_slice(shas);
    args.extend_from_slice(after);
    let (bytes, truncated) = run(repo, &args, false).await.ok()?;
    if truncated {
        return None;
    }
    Some(
        String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}
