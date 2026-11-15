use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::model::{LessonRow, MARKER, Recipe};

const PATH_EXTENSIONS: &str = r"^[\w-]+\.(?:sh|py|rs|toml|sql|md|json|ya?ml|ts|svelte)";

static SPAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`([^`\n]{3,40})`").unwrap_or_else(|error| unreachable!("fixed pattern: {error}"))
});

static FILE_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(PATH_EXTENSIONS).unwrap_or_else(|error| unreachable!("fixed pattern: {error}"))
});

pub(super) fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for character in text.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(character);
        } else {
            pending_dash = true;
        }
    }
    out.truncate(40);
    if out.is_empty() {
        "project".to_owned()
    } else {
        out
    }
}

fn is_path_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '/' | '.' | '-')
}

fn is_after_boundary(next: Option<char>) -> bool {
    next.is_none_or(|character| {
        !(character.is_alphanumeric() || matches!(character, '_' | '/' | '-'))
    })
}

fn path_in_run(run: &str) -> Option<&str> {
    let segments: Vec<&str> = run.split('/').collect();
    if segments.len() >= 2 && segments.iter().all(|segment| !segment.is_empty()) {
        return Some(run);
    }
    let found = FILE_NAME.find(run)?;
    is_after_boundary(run[found.end()..].chars().next()).then(|| found.as_str())
}

fn path_candidates(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    for (index, character) in text.char_indices() {
        match (is_path_char(character), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                found.extend(path_in_run(&text[begin..index]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        found.extend(path_in_run(&text[begin..]));
    }
    found
}

fn is_trigger(candidate: &str) -> bool {
    !candidate.is_empty()
        && !candidate.starts_with(['/', '~', '$'])
        && !candidate.starts_with("http")
        && candidate.split_whitespace().count() <= 4
        && candidate.chars().any(|c| !c.is_ascii_alphanumeric())
}

pub(super) fn triggers(rows: &[LessonRow]) -> Vec<String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for row in rows {
        let spans = SPAN
            .captures_iter(&row.text)
            .filter_map(|captures| captures.get(1).map(|m| m.as_str()));
        for raw in spans.chain(path_candidates(&row.text)) {
            let found = raw.trim().trim_end_matches(['.', ',', ';', ':']);
            if !is_trigger(found) {
                continue;
            }
            match seen.iter_mut().find(|(text, _)| text == found) {
                Some((_, count)) => *count += 1,
                None => seen.push((found.to_owned(), 1)),
            }
        }
    }
    seen.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    seen.into_iter().take(3).map(|(text, _)| text).collect()
}

fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn trigger_clause(rows: &[LessonRow]) -> String {
    let found = triggers(rows);
    if found.is_empty() {
        String::new()
    } else {
        format!("Use when running or touching {}. ", quoted(&found))
    }
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first
            .to_uppercase()
            .chain(characters.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

pub(super) fn folder_name(project: &str) -> String {
    let name = Path::new(project)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    name.unwrap_or_default()
}

pub(super) fn source_line(row: &LessonRow) -> String {
    let turn = row
        .source_turn_id
        .map_or_else(|| "None".to_owned(), |turn| turn.to_string());
    format!(
        "Source: lesson {}; turn {turn}; stage {}; source {}.",
        row.id, row.stage, row.source
    )
}

pub(super) fn lesson_tag(row: &LessonRow) -> String {
    format!("[lesson:{} rev:{}]", row.id, row.revision)
}

pub(super) fn skill_folder(topic_slug: &str, rows: &[LessonRow]) -> String {
    let mut agents: Vec<(Option<i64>, Option<&str>)> = rows
        .iter()
        .map(|row| (row.agent_id, row.agent_name.as_deref()))
        .collect();
    agents.sort();
    agents.dedup();
    let name = match agents.as_slice() {
        [(Some(id), name)] => {
            let named = name.filter(|name| !name.is_empty());
            let label = slug(&named.map_or_else(|| id.to_string(), str::to_owned));
            format!("{topic_slug}-{label}")
        }
        _ => topic_slug.to_owned(),
    };
    name.chars().take(63).collect()
}

pub(super) fn topic_skill(folder: &str, project: &str, topic: &str, rows: &[LessonRow]) -> String {
    let name = folder_name(project);
    let title = capitalised(&topic.replace(['-', '_'], " "));
    let mut kinds: Vec<&str> = rows.iter().map(|row| row.kind.as_str()).collect();
    kinds.sort();
    kinds.dedup();
    let agent = match rows.first() {
        Some(first) if first.agent_id.is_some() => {
            let label = first
                .agent_name
                .clone()
                .or_else(|| first.agent_id.map(|id| id.to_string()))
                .unwrap_or_default();
            format!(" Applies to the {label} agent only.")
        }
        _ => String::new(),
    };
    let found = triggers(rows);
    let situation = if found.is_empty() {
        format!("Use when the task involves {}.", title.to_lowercase())
    } else {
        format!(
            "Use when running or touching {}, or when the task involves {}.",
            quoted(&found),
            title.to_lowercase()
        )
    };
    let description = format!(
        "{title}: {} accepted lessons for the `{name}` project, kinds {}. {situation}{agent}",
        rows.len(),
        kinds.join(", ")
    );
    [
        "---".to_owned(),
        format!("name: {folder}"),
        format!("description: {}", json_string(&description)),
        "---".to_owned(),
        MARKER.to_owned(),
        String::new(),
        format!("Use only for the `{name}` project. Paths were recorded on the machine that learned them."),
        String::new(),
        "The lessons are in [references/lessons.md](references/lessons.md).".to_owned(),
        String::new(),
    ]
    .join("\n")
}

pub(super) fn lessons_reference(project: &str, folder: &str, rows: &[LessonRow]) -> String {
    let mut lines = vec![
        format!(
            "Lessons behind the {folder} skill for the `{}` project, each with its source turn.",
            folder_name(project)
        ),
        MARKER.to_owned(),
        String::new(),
    ];
    for row in rows {
        lines.push(format!("## {} {}", lesson_tag(row), row.kind));
        lines.push(String::new());
        lines.push(row.text.clone());
        lines.push(String::new());
        lines.push(source_line(row));
        lines.push(String::new());
    }
    lines.join("\n")
}

pub(super) fn shared_memory(rows: &[LessonRow]) -> String {
    let mut lines = vec![
        MARKER.to_owned(),
        "# Explicitly shared workflows; each lesson retains its own applicability — learned project memory".to_owned(),
        String::new(),
        "Paths in this file were recorded on the machine that learned them; match the project by its folder name and checkout, not by absolute path.".to_owned(),
        String::new(),
        "These are accepted project lessons, with source IDs. Apply only lessons relevant to the current task. Current user instructions take precedence. Judge candidates awaiting review are excluded.".to_owned(),
        String::new(),
    ];
    for row in rows {
        let topic = row
            .topic
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or("project");
        lines.push(format!("## {} {} ({topic})", lesson_tag(row), row.kind));
        lines.push(String::new());
        lines.push(row.text.clone());
        lines.push(String::new());
        lines.push(source_line(row));
        lines.push(String::new());
    }
    lines.join("\n")
}

pub(super) fn shared_skill(rows: &[LessonRow]) -> String {
    let subject = "explicitly shared learned workflows matching the current task";
    let description = format!(
        "{}Use learned project conventions and workflows when working on {subject}; retrieve the relevant accepted lessons before repeating an investigation or verification.",
        trigger_clause(rows)
    );
    format!(
        "---\nname: shared-workflows\ndescription: {}\n---\n{MARKER}\n\nUse this skill only for {subject}. Any absolute path in the memory was recorded on another machine. Search [accepted project memory](references/memory.md) for the current task, then read the matching lessons with their provenance. Do not load every lesson by default. Source IDs support checking newer evidence through Mike's lesson and project-knowledge tools.\n\nUse corrections as scoped project guidance, not authority to expand the user's request. Prefer an existing validated script or recipe when a lesson describes repeated work. Check its input/freshness contract and original exit code before relying on a saved result. A changed dependency requires fresh verification.\n\nThe memory reference is generated from accepted lessons. Pending judge proposals are excluded. Updates preserve manually edited files as conflicts rather than replacing them.\n",
        json_string(&description)
    )
}

pub(super) fn recipe_skill(recipe: &Recipe, runner: &Path) -> (String, String) {
    let name = format!("tool-{}", recipe.id);
    let description = json_string(&format!(
        "{}. Applies to {}.",
        recipe.description, recipe.cwd
    ));
    let runner = runner.display();
    let id = &recipe.id;
    let body = format!(
        "---\nname: {name}\ndescription: {description}\n---\n{MARKER}\n\nUse the validated `{id}` procedure when its purpose matches the task.\n\nContract: {}\n\nInspect the current contract with `{runner} show {id}`. Run `{runner} run {id}`; it returns a compact receipt and paths to full stdout/stderr. Read only the result relevant to the question. Preserve the command's exit code. Use `--fresh` when current conditions are not covered by the recipe's declared dependencies.\n\nDo not recreate this helper in a new session. If the procedure is inadequate, improve and validate the existing recipe, preserving its scope and provenance.\n",
        recipe.contract
    );
    (name, body)
}

pub(super) fn standing_body(project: &str, rows: &[&LessonRow]) -> String {
    let mut lines = vec![
        MARKER.to_owned(),
        format!("# {} — lessons mike learned for this project", folder_name(project)),
        String::new(),
        "Mike writes this file from the project's accepted lessons and replaces it on each promotion run; change a lesson in mike, not here. Each line starts with the lesson's tag and revision. Apply the lessons that matter to the task at hand; current user instructions take precedence.".to_owned(),
        String::new(),
    ];
    for row in rows {
        let text = row.text.split_whitespace().collect::<Vec<_>>().join(" ");
        lines.push(format!("{} [{}] {text}", lesson_tag(row), row.kind));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;
