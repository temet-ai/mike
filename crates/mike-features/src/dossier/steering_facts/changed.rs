use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use super::observation::CallFact;
use super::read_back::written_paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Code,
    Test,
    Doc,
    Config,
    Ui,
    Style,
    Generated,
    Other,
}

impl PathKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Test => "test",
            Self::Doc => "doc",
            Self::Config => "config",
            Self::Ui => "UI",
            Self::Style => "style",
            Self::Generated => "generated",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedPath {
    pub path: String,
    pub kind: PathKind,
    pub added: u32,
    pub removed: u32,
    pub committed: Option<bool>,
}

const GENERATED_PARTS: &[&str] = &[
    "/target/",
    "/dist/",
    "/build/",
    "node_modules/",
    "/.sqlx/",
    "/generated/",
    "__snapshots__/",
];
const TEST_PARTS: &[&str] = &["/tests/", "/test/", "/__tests__/", "/spec/", "/e2e/"];
const TEST_NAMES: &[&str] = &["_test.", ".test.", ".spec.", "_tests.", "_spec."];
const UI_EXT: &[&str] = &["svelte", "vue", "tsx", "jsx", "html", "astro"];
const STYLE_EXT: &[&str] = &["css", "scss", "sass", "less"];
const DOC_EXT: &[&str] = &["md", "mdx", "rst", "adoc", "txt"];
const CONFIG_EXT: &[&str] = &[
    "toml", "yaml", "yml", "json", "ini", "conf", "cfg", "env", "plist",
];
const CONFIG_NAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    ".gitignore",
    ".editorconfig",
    "justfile",
];
const CODE_EXT: &[&str] = &[
    "rs", "ts", "js", "mjs", "cjs", "py", "go", "java", "kt", "swift", "c", "cc", "cpp", "h",
    "hpp", "rb", "sh", "bash", "zsh", "sql", "php", "cs", "scala", "lua", "ex", "exs",
];

pub fn path_kind(path: &str) -> PathKind {
    let lower = format!("/{}", path.to_ascii_lowercase());
    let name = lower.rsplit('/').next().unwrap_or("");
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    if GENERATED_PARTS.iter().any(|p| lower.contains(p))
        || name.ends_with(".lock")
        || name.contains(".min.")
        || ext == "snap"
    {
        return PathKind::Generated;
    }
    if TEST_PARTS.iter().any(|p| lower.contains(p))
        || TEST_NAMES.iter().any(|p| name.contains(p))
        || name.starts_with("test_")
    {
        return PathKind::Test;
    }
    if UI_EXT.contains(&ext) || name.contains(".stories.") {
        return PathKind::Ui;
    }
    if STYLE_EXT.contains(&ext) {
        return PathKind::Style;
    }
    if DOC_EXT.contains(&ext) || lower.contains("/docs/") {
        return PathKind::Doc;
    }
    if CONFIG_EXT.contains(&ext)
        || CONFIG_NAMES.contains(&name)
        || lower.contains("/.github/")
        || lower.contains("/.claude/")
    {
        return PathKind::Config;
    }
    if CODE_EXT.contains(&ext) {
        return PathKind::Code;
    }
    PathKind::Other
}

fn lines(v: Option<&Value>) -> u32 {
    v.and_then(Value::as_str)
        .map_or(0, |s| u32::try_from(s.lines().count()).unwrap_or(u32::MAX))
}

fn edit_counts(edit: &Value) -> (u32, u32) {
    (lines(edit.get("new_string")), lines(edit.get("old_string")))
}

pub fn line_counts(call: &CallFact) -> (u32, u32) {
    let input = &call.input;
    match call.tool_name.as_str() {
        "Edit" => edit_counts(input),
        "Write" => (lines(input.get("content")), 0),
        "MultiEdit" => input
            .get("edits")
            .and_then(Value::as_array)
            .map_or((0, 0), |edits| {
                edits
                    .iter()
                    .map(edit_counts)
                    .fold((0, 0), |(a, r), (x, y)| {
                        (a.saturating_add(x), r.saturating_add(y))
                    })
            }),
        "NotebookEdit" => (lines(input.get("new_source")), 0),
        _ => (0, 0),
    }
}

pub fn changed_paths(window: &[CallFact]) -> Vec<ChangedPath> {
    let mut by_path: BTreeMap<String, (u32, u32, usize)> = BTreeMap::new();
    for (order, call) in window.iter().filter(|c| c.has_result).enumerate() {
        let (added, removed) = line_counts(call);
        for path in written_paths(call) {
            let entry = by_path.entry(path).or_insert((0, 0, order));
            entry.0 = entry.0.saturating_add(added);
            entry.1 = entry.1.saturating_add(removed);
            entry.2 = order;
        }
    }
    let mut rows: Vec<(usize, ChangedPath)> = by_path
        .into_iter()
        .map(|(path, (added, removed, order))| {
            let kind = path_kind(&path);
            (
                order,
                ChangedPath {
                    path,
                    kind,
                    added,
                    removed,
                    committed: None,
                },
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    rows.into_iter().map(|(_, c)| c).collect()
}

fn relative<'a>(path: &'a str, checkout: &str) -> Option<&'a str> {
    let root = checkout.trim_end_matches('/');
    path.strip_prefix(root)?.strip_prefix('/')
}

pub fn mark_committed(changed: &mut [ChangedPath], checkout: &str, open: &[String]) {
    for c in changed.iter_mut() {
        let Some(rel) = relative(&c.path, checkout).map(str::to_owned) else {
            continue;
        };
        c.committed = Some(!open.iter().any(|o| o == &rel));
        c.path = rel;
    }
}

fn shown_path(path: &str) -> String {
    if !path.starts_with('/') {
        return path.to_owned();
    }
    let parts: Vec<&str> = path.rsplit('/').take(4).collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

pub fn describe(c: &ChangedPath) -> String {
    let state = match c.committed {
        Some(true) => ", committed",
        Some(false) => ", uncommitted",
        None => "",
    };
    format!(
        "{} ({}, +{} \u{2212}{}{state})",
        shown_path(&c.path),
        c.kind.label(),
        c.added,
        c.removed
    )
}

pub fn summary(changed: &[ChangedPath]) -> Option<String> {
    let first = changed.first()?;
    let rest = changed.len() - 1;
    Some(match rest {
        0 => describe(first),
        1 => format!("{} and 1 more", describe(first)),
        n => format!("{} and {n} more", describe(first)),
    })
}
