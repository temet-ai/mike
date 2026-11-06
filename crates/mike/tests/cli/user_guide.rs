use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::Subcommand as _;

const FIX: &str = "The guide is docs/user-guide; the user-guide-writer agent changes it.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> anyhow::Result<String> {
    let full = repo_root().join(path);
    std::fs::read_to_string(&full).with_context(|| full.display().to_string())
}

fn guide_pages_by_file_name() -> anyhow::Result<Vec<(String, String)>> {
    let dir = repo_root().join("docs/user-guide");
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| dir.display().to_string())? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            let text = read(&format!("docs/user-guide/{name}"))?;
            pages.push((name, text));
        }
    }
    pages.sort();
    Ok(pages)
}

fn fenced_lines_and_code_spans(page: &str) -> Vec<&str> {
    let mut texts = Vec::new();
    let mut fenced = false;
    for line in page.lines() {
        let start = line.trim_start();
        if start.starts_with("```") || start.starts_with("~~~") {
            fenced = !fenced;
        } else if fenced {
            texts.push(line.trim());
        } else {
            texts.extend(line.split('`').skip(1).step_by(2));
        }
    }
    texts
}

fn mike_commands() -> BTreeSet<String> {
    let app = mike::cli::Command::augment_subcommands(clap::Command::new("mike"));
    let names: BTreeSet<String> = app
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect();
    assert!(
        names.len() > 20,
        "clap lists only {} commands of mike",
        names.len()
    );
    names
}

fn command_typed_after_mike(text: &str) -> Option<&str> {
    let text = text.strip_prefix("$ ").unwrap_or(text);
    let word = text.strip_prefix("mike ")?.split_whitespace().next()?;
    let is_name = word.starts_with(|c: char| c.is_ascii_lowercase())
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    is_name.then_some(word)
}

fn prefixes_named_with_a_star<'a>(code: &[&'a str]) -> Vec<&'a str> {
    code.iter()
        .filter_map(|text| text.strip_suffix('*'))
        .filter(|prefix| prefix.ends_with('_'))
        .collect()
}

#[test]
fn command_line_page_for_every_mike_command_names_it() -> anyhow::Result<()> {
    let page = read("docs/user-guide/command-line.md")?;
    let named: BTreeSet<&str> = fenced_lines_and_code_spans(&page)
        .into_iter()
        .flat_map(str::split_whitespace)
        .collect();
    let missing: Vec<String> = mike_commands()
        .into_iter()
        .filter(|name| !named.contains(name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/user-guide/command-line.md lists every `mike` command and does not name: {missing:?}. {FIX}"
    );
    Ok(())
}

#[test]
fn guide_code_starting_with_mike_names_an_existing_command() -> anyhow::Result<()> {
    let mut known = mike_commands();
    known.insert("help".to_owned());
    let mut seen = 0;
    let mut unknown = Vec::new();
    for (name, page) in guide_pages_by_file_name()? {
        let typed = fenced_lines_and_code_spans(&page)
            .into_iter()
            .filter_map(command_typed_after_mike);
        for word in typed {
            seen += 1;
            if !known.contains(word) {
                unknown.push(format!("{name}: mike {word}"));
            }
        }
    }
    assert!(
        seen > 100,
        "found only {seen} `mike <command>` texts in the guide; the scan no longer sees them"
    );
    assert!(
        unknown.is_empty(),
        "the guide shows commands mike does not have: {unknown:?}. {FIX}"
    );
    Ok(())
}

#[test]
fn settings_page_for_every_feature_switch_names_it() -> anyhow::Result<()> {
    let page = read("docs/user-guide/settings.md")?;
    let code = fenced_lines_and_code_spans(&page);
    let words: BTreeSet<&str> = code
        .iter()
        .flat_map(|text| text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')))
        .collect();
    let prefixes = prefixes_named_with_a_star(&code);
    let switches = mike::feature_toggles::TOGGLE_META;
    assert!(
        switches.len() > 20,
        "mike lists only {} feature switches",
        switches.len()
    );
    let missing: Vec<&str> = switches
        .iter()
        .map(|meta| meta.key)
        .filter(|key| !words.contains(key))
        .filter(|key| !prefixes.iter().any(|prefix| key.starts_with(prefix)))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/user-guide/settings.md does not name these feature switches: {missing:?}. {FIX}"
    );
    Ok(())
}

#[test]
fn pages_page_for_every_menu_entry_names_it() -> anyhow::Result<()> {
    let nav = read("frontend/src/lib/components/shell/DestinationNav.svelte")?;
    let labels: BTreeSet<&str> = nav
        .lines()
        .filter_map(|line| line.split_once("label: '")?.1.split_once('\''))
        .map(|(label, _)| label)
        .collect();
    assert!(
        labels.len() > 20,
        "found only {} menu labels in DestinationNav.svelte; the scan no longer sees them",
        labels.len()
    );
    let page = read("docs/user-guide/pages.md")?;
    let missing: Vec<&str> = labels
        .into_iter()
        .filter(|label| !page.contains(label))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/user-guide/pages.md does not name these entries of the web UI's menu: {missing:?}. {FIX}"
    );
    Ok(())
}
