use std::path::Path;

use anyhow::Context;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub thread: String,
    pub repo: String,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub raw: String,
}

pub struct RegistryRead {
    pub entries: Vec<RegistryEntry>,
    pub problems: Vec<String>,
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

pub fn default_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join(".claude/threads/registry.jsonl"))
}

pub fn read(path: &Path) -> anyhow::Result<RegistryRead> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read the thread registry {}", path.display()))?;
    let mut read = RegistryRead {
        entries: Vec::new(),
        problems: Vec::new(),
    };
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let number = index + 1;
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            read.problems
                .push(format!("{}:{number} is not JSON", path.display()));
            continue;
        };
        let (Some(thread), Some(repo)) = (text_field(&value, "thread"), text_field(&value, "repo"))
        else {
            read.problems.push(format!(
                "{}:{number} has no `thread` or no `repo` field",
                path.display()
            ));
            continue;
        };
        read.entries.push(RegistryEntry {
            thread,
            repo: repo.trim_end_matches('/').to_owned(),
            status: text_field(&value, "status"),
            branch: text_field(&value, "branch"),
            worktree: text_field(&value, "worktree"),
            raw: line.trim().to_owned(),
        });
    }
    Ok(read)
}
