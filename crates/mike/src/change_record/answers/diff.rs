use serde::Serialize;

use super::Caller;
use super::diff_git::{relative_path, resolve_commit, run, text, valid_object};
use crate::change_record::{ChangeEvent, ChangeKind, ChangeQuery};
use crate::store::persistence::PersistHandle;

const EVENTS_LIMIT: i64 = 1000;
pub const DEFAULT_DIFF_LINES: usize = 400;
pub const MAX_DIFF_LINES: usize = 2000;

#[derive(Debug, Serialize)]
pub struct DiffAnswer {
    pub source: &'static str,
    pub commit: Option<String>,
    pub paths: Vec<String>,
    pub unified_diff: String,
    pub truncated: bool,
    pub unavailable: Vec<String>,
}

fn empty(source: &'static str) -> DiffAnswer {
    DiffAnswer {
        source,
        commit: None,
        paths: Vec::new(),
        unified_diff: String::new(),
        truncated: false,
        unavailable: Vec::new(),
    }
}

pub async fn commit_diff(
    repo: &str,
    commit: &str,
    path: Option<&str>,
    limit: usize,
) -> Result<DiffAnswer, String> {
    let commit = resolve_commit(repo, commit).await?;
    let mut args = vec![
        "show",
        "--format=",
        "--first-parent",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        &commit,
        "--",
    ];
    if let Some(path) = path {
        args.push(relative_path(path)?);
    }
    let (bytes, capped) = run(repo, &args, false).await?;
    let (unified_diff, truncated) = text(&bytes, limit)?;
    Ok(DiffAnswer {
        source: "git_commit_first_parent",
        commit: Some(commit),
        paths: path.into_iter().map(str::to_string).collect(),
        unified_diff,
        truncated: capped || truncated,
        unavailable: Vec::new(),
    })
}

pub async fn own_diff(
    persist: &PersistHandle,
    caller: &Caller,
    limit: usize,
) -> Result<DiffAnswer, String> {
    let open = persist
        .open_changes(&caller.checkout)
        .await
        .map_err(|e| e.to_string())?;
    let mut answer = empty("working_tree_at_recorded_owned_paths");
    answer.paths = open
        .into_iter()
        .filter(|p| p.session_id == Some(caller.session_id))
        .map(|p| p.path)
        .collect();
    answer.paths.sort();
    answer.paths.dedup();
    for path in answer.paths.clone() {
        relative_path(&path)?;
        let tracked = run(
            &caller.checkout,
            &["ls-files", "--error-unmatch", "--", &path],
            false,
        )
        .await
        .is_ok();
        let args = if tracked {
            vec![
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "HEAD",
                "--",
                &path,
            ]
        } else {
            vec![
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--",
                "/dev/null",
                &path,
            ]
        };
        match run(&caller.checkout, &args, !tracked).await {
            Ok((bytes, capped)) => append(&mut answer, &bytes, capped, limit)?,
            Err(error) => answer.unavailable.push(format!("{path}: {error}")),
        }
        if answer.truncated {
            break;
        }
    }
    Ok(answer)
}

pub async fn path_diff(
    persist: &PersistHandle,
    caller: &Caller,
    path: &str,
    limit: usize,
) -> Result<DiffAnswer, String> {
    relative_path(path)?;
    let place = persist
        .change_place(caller.session_id, &caller.checkout)
        .await
        .map_err(|e| e.to_string())?;
    let query = ChangeQuery {
        checkout: caller.checkout.clone(),
        after_id: place,
        path_prefixes: vec![path.to_string()],
        exclude_session: None,
        kinds: vec![ChangeKind::Edit, ChangeKind::ShellChange],
        limit: EVENTS_LIMIT + 1,
    };
    let events = persist
        .change_events_after(&query)
        .await
        .map_err(|e| e.to_string())?;
    let mut answer = empty("recorded_blobs_since_place");
    answer.paths.push(path.to_string());
    if events.len() > EVENTS_LIMIT as usize {
        answer.truncated = true;
        answer
            .unavailable
            .push("record window exceeds 1000 events; baseline is incomplete".into());
        return Ok(answer);
    }
    let Some((before, after)) = endpoints(&events, path) else {
        return Ok(answer);
    };
    match blob_diff(&caller.checkout, before, after).await {
        Ok((bytes, capped)) => append(&mut answer, &bytes, capped, limit)?,
        Err(error) => answer.unavailable.push(error),
    }
    Ok(answer)
}

fn endpoints<'a>(
    events: &'a [ChangeEvent],
    path: &str,
) -> Option<(Option<&'a str>, Option<&'a str>)> {
    let mut matching: Vec<_> = events
        .iter()
        .filter(|e| e.paths.iter().any(|p| p == path))
        .collect();
    matching.sort_by_key(|e| e.id);
    Some((
        matching.first()?.before_sha.as_deref(),
        matching.last()?.after_sha.as_deref(),
    ))
}

async fn blob(repo: &str, sha: Option<&str>) -> Result<Vec<u8>, String> {
    let Some(sha) = sha else {
        return Ok(Vec::new());
    };
    if !valid_object(sha) {
        return Err("record contains an invalid blob id".into());
    }
    let (bytes, capped) = run(repo, &["cat-file", "blob", sha], false).await?;
    if capped {
        return Err("recorded blob exceeds the two-MiB read limit".into());
    }
    Ok(bytes)
}

pub(super) async fn blob_diff(
    repo: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<(Vec<u8>, bool), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let old = dir.path().join("before");
    let new = dir.path().join("after");
    tokio::fs::write(&old, blob(repo, before).await?)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::write(&new, blob(repo, after).await?)
        .await
        .map_err(|e| e.to_string())?;
    let old_name = old.to_string_lossy();
    let new_name = new.to_string_lossy();
    let old_arg = before.map_or("/dev/null", |_| old_name.as_ref());
    let new_arg = after.map_or("/dev/null", |_| new_name.as_ref());
    run(
        repo,
        &[
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--",
            old_arg,
            new_arg,
        ],
        true,
    )
    .await
}

fn append(answer: &mut DiffAnswer, bytes: &[u8], capped: bool, limit: usize) -> Result<(), String> {
    let remaining = limit.saturating_sub(answer.unified_diff.lines().count());
    let (addition, truncated) = text(bytes, remaining)?;
    answer.unified_diff.push_str(&addition);
    answer.truncated |= capped || truncated;
    Ok(())
}
