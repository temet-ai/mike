use std::path::{Component, Path};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_CHECKED_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OnDisk {
    NotChecked(&'static str),
    Matches,
    Differs {
        indexed_characters: usize,
        disk_characters: usize,
    },
    Missing,
    Unreadable(String),
}

pub(super) struct Indexed<'a> {
    pub root: &'a str,
    pub relative_path: &'a str,
    pub content_hash: &'a str,
    pub body: &'a str,
}

fn stays_inside(relative_path: &str) -> bool {
    !relative_path.is_empty()
        && Path::new(relative_path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub(super) async fn inspect(indexed: &Indexed<'_>) -> OnDisk {
    if !stays_inside(indexed.relative_path) {
        return OnDisk::NotChecked("the indexed path is not relative to the source root");
    }
    let root = Path::new(indexed.root);
    if !tokio::fs::metadata(root)
        .await
        .is_ok_and(|meta| meta.is_dir())
    {
        return OnDisk::NotChecked("the source root is not on this machine");
    }
    let path = root.join(indexed.relative_path);
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.len() > MAX_CHECKED_BYTES => {
            return OnDisk::NotChecked("the file is too large to compare");
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return OnDisk::Missing,
        Err(error) => return OnDisk::Unreadable(error.to_string()),
    }
    match tokio::fs::read(&path).await {
        Ok(bytes) => compare(indexed, &bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => OnDisk::Missing,
        Err(error) => OnDisk::Unreadable(error.to_string()),
    }
}

fn compare(indexed: &Indexed<'_>, bytes: &[u8]) -> OnDisk {
    if hex::encode(Sha256::digest(bytes)) == indexed.content_hash {
        return OnDisk::Matches;
    }
    OnDisk::Differs {
        indexed_characters: indexed.body.chars().count(),
        disk_characters: String::from_utf8_lossy(bytes).chars().count(),
    }
}

fn staleness(state: &OnDisk) -> Option<String> {
    match state {
        OnDisk::Differs {
            indexed_characters,
            disk_characters,
        } => Some(format!(
            "the file on disk has {disk_characters} characters and the indexed copy {indexed_characters}; the index has not caught up with the file"
        )),
        OnDisk::Missing => Some("the file no longer exists in the source root".to_owned()),
        OnDisk::Unreadable(error) => Some(format!("the file on disk cannot be read: {error}")),
        OnDisk::NotChecked(_) | OnDisk::Matches => None,
    }
}

fn describe(state: &OnDisk) -> Value {
    match state {
        OnDisk::NotChecked(reason) => json!({"state": "not_checked", "reason": reason}),
        OnDisk::Matches => json!({"state": "matches"}),
        OnDisk::Differs {
            indexed_characters,
            disk_characters,
        } => json!({"state": "differs", "indexed_characters": indexed_characters,
            "disk_characters": disk_characters}),
        OnDisk::Missing => json!({"state": "missing"}),
        OnDisk::Unreadable(error) => json!({"state": "unreadable", "error": error}),
    }
}

pub(super) fn apply(read: &mut Value, state: &OnDisk) {
    read["on_disk"] = describe(state);
    let Some(reason) = staleness(state) else {
        return;
    };
    let Some(freshness) = read.get_mut("freshness").and_then(Value::as_object_mut) else {
        return;
    };
    freshness.insert("state".to_owned(), json!("stale"));
    let reasons = freshness
        .entry("reasons")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(list) = reasons.as_array_mut() {
        list.push(json!(reason));
    }
}

pub(super) fn checkable(read: &Value) -> Option<Indexed<'_>> {
    if read.get("current").and_then(Value::as_bool) != Some(true)
        || read.get("deleted").and_then(Value::as_bool) != Some(false)
    {
        return None;
    }
    Some(Indexed {
        root: read.get("root_path")?.as_str()?,
        relative_path: read.get("path")?.as_str()?,
        content_hash: read.get("content_hash")?.as_str()?,
        body: read.get("body")?.as_str()?,
    })
}

pub(super) async fn annotate(read: &mut Value) {
    let state = match checkable(read) {
        Some(indexed) => inspect(&indexed).await,
        None if read.get("freshness").is_some() => {
            OnDisk::NotChecked("only the current, undeleted revision is compared with the file")
        }
        None => return,
    };
    apply(read, &state);
}
