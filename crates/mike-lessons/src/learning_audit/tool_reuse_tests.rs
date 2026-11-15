use chrono::{TimeZone, Utc};
use serde_json::json;

use super::fetch::Window;
use super::tool_reuse::{events_in, export, render_recipes};

fn window() -> anyhow::Result<Window> {
    let instant = |day| {
        Utc.with_ymd_and_hms(2026, 11, day, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow::anyhow!("invalid test instant"))
    };
    Ok(Window {
        start: instant(2)?,
        end: instant(9)?,
    })
}

#[test]
fn events_in_keeps_the_window_start_and_drops_its_end() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let lines = [
        json!({"at": "2026-11-01T23:59:59+00:00", "kind": "before"}),
        json!({"at": "2026-11-02T00:00:00+00:00", "kind": "start"}),
        json!({"at": "2026-11-05T12:00:00.250000+00:00", "kind": "inside"}),
        json!({"at": "2026-11-09T00:00:00+00:00", "kind": "end"}),
        json!({"kind": "undated"}),
    ]
    .map(|event| event.to_string());
    std::fs::write(
        directory.path().join("events.jsonl"),
        format!("{}\nnot json\n\n", lines.join("\n")),
    )?;
    let kept: Vec<String> = events_in(directory.path(), &window()?)?
        .iter()
        .map(|event| event["kind"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(kept, ["start", "inside"]);
    Ok(())
}

#[test]
fn events_in_without_a_log_is_empty() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(events_in(directory.path(), &window()?)?.is_empty());
    Ok(())
}

#[test]
fn render_recipes_lists_each_recipe_under_the_header() {
    let text = render_recipes(&[json!({
        "id": "cargo-check", "description": "Check the crate", "cwd": "/work/mike",
        "contract": "exit 0 means it compiles", "argv": ["cargo", "check"],
        "dependencies": ["Cargo.toml"], "ttl_seconds": 600,
    })]);
    assert!(text.starts_with("# Validated reusable tool recipes\n\n"));
    for expected in [
        "## cargo-check",
        "Working directory: `/work/mike`",
        "Contract: exit 0 means it compiles",
        "Command: `[\"cargo\",\"check\"]`",
        "Dependencies: [\"Cargo.toml\"]; TTL: 600s.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn export_reads_recipes_in_file_name_order() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let recipes = directory.path().join("recipes");
    std::fs::create_dir(&recipes)?;
    std::fs::write(
        recipes.join("b-second.json"),
        json!({"id": "b-second"}).to_string(),
    )?;
    std::fs::write(
        recipes.join("a-first.json"),
        json!({"id": "a-first"}).to_string(),
    )?;
    std::fs::write(recipes.join("ignored.txt"), "x")?;
    let target = directory.path().join("tool-recipes.md");
    export(directory.path(), &target)?;
    let text = std::fs::read_to_string(target)?;
    let first = text.find("## a-first");
    let second = text.find("## b-second");
    assert!(first.is_some() && second.is_some() && first < second);
    Ok(())
}
