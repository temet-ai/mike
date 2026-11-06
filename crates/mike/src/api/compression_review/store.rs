use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn locked(path: &Path) -> Result<File> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    ensure!(
        path.metadata()?.permissions().mode() & 0o077 == 0,
        "Queue directory must be private"
    );
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .mode(0o600)
        .open(path.join("events.jsonl"))?;
    file.lock()?;
    ensure!(
        file.metadata()?.permissions().mode() & 0o077 == 0,
        "Queue file must be private"
    );
    Ok(file)
}

pub(super) fn events(file: &File) -> Result<Vec<Value>> {
    BufReader::new(file)
        .lines()
        .filter_map(|line| match line {
            Ok(line) if line.trim().is_empty() => None,
            value => Some(value),
        })
        .map(|line| Ok(serde_json::from_str(&line?)?))
        .collect()
}

pub(super) fn items(events: &[Value]) -> Result<Vec<Value>> {
    let mut items = BTreeMap::<String, Value>::new();
    for event in events {
        let id = event["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Event lacks identity"))?;
        if event["action"] == "flag" {
            items.entry(id.to_owned()).or_insert_with(|| {
                json!({
                    "id": id, "target": event["target"], "status": "open",
                    "created_at": event["at"], "history": [], "capture_verified": false
                })
            });
        }
        let item = items
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Review lacks flag"))?;
        item["updated_at"] = event["at"].clone();
        if event["action"] == "review" {
            item["status"] = event["status"].clone();
        }
        item["history"]
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid history"))?
            .push(event.clone());
    }
    Ok(items.into_values().collect())
}

pub(super) fn target_id(kind: &str, request_id: i64, tool_id: Option<i64>) -> String {
    let tool = tool_id.map_or_else(|| "null".to_owned(), |value| value.to_string());
    let canonical =
        format!("{{\"kind\": \"{kind}\", \"request_id\": {request_id}, \"tool_use_id\": {tool}}}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

pub(super) fn append(file: &mut File, mut event: Value) -> Result<()> {
    event["at"] = json!(chrono::Utc::now().to_rfc3339());
    event["version"] = json!(1);
    file.seek(SeekFrom::End(0))?;
    serde_json::to_writer(&mut *file, &event)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
