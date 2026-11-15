use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::fetch::Window;
use super::rows::{show, text};

const EVENTS_FILE: &str = "events.jsonl";
const RECIPES_DIRECTORY: &str = "recipes";

fn event_time(event: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(event.get("at")?.as_str()?)
        .ok()
        .map(|moment| moment.with_timezone(&Utc))
}

pub(super) fn events_in(directory: &Path, window: &Window) -> Result<Vec<Value>> {
    let path = directory.join(EVENTS_FILE);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    Ok(content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event_time(event).is_some_and(|at| window.start <= at && at < window.end))
        .collect())
}

fn recipes(directory: &Path) -> Result<Vec<Value>> {
    let folder = directory.join(RECIPES_DIRECTORY);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", folder.display()));
        }
    };
    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|file| {
            let bytes = std::fs::read(file)
                .with_context(|| format!("cannot read recipe {}", file.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("recipe {} is not valid JSON", file.display()))
        })
        .collect()
}

pub(super) fn render_recipes(recipes: &[Value]) -> String {
    let mut lines = vec![
        "# Validated reusable tool recipes".to_owned(),
        String::new(),
        concat!(
            "Discover with `scripts/tool-reuse list`; inspect contracts with `show ID`; execute with `run ID`. ",
            "Results report the original exit code and full stdout/stderr paths. Dependencies changing invalidate reuse."
        )
        .to_owned(),
        String::new(),
    ];
    for recipe in recipes {
        lines.extend([
            format!("## {}", text(recipe, "id")),
            String::new(),
            text(recipe, "description"),
            String::new(),
            format!("Working directory: `{}`", text(recipe, "cwd")),
            String::new(),
            format!("Contract: {}", text(recipe, "contract")),
            String::new(),
            format!("Command: `{}`", show(recipe.get("argv"))),
            String::new(),
            format!(
                "Dependencies: {}; TTL: {}s.",
                show(recipe.get("dependencies")),
                text(recipe, "ttl_seconds")
            ),
            String::new(),
        ]);
    }
    lines.join("\n")
}

pub(super) fn export(directory: &Path, target: &Path) -> Result<()> {
    let document = render_recipes(&recipes(directory)?);
    std::fs::write(target, document).with_context(|| format!("cannot write {}", target.display()))
}
