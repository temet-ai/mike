use serde::{Deserialize, Serialize};

use super::watch::git;

const MOST: usize = 200;
const KEYWORDS: &str = "fn|struct|enum|trait|type|const|static|mod|impl|macro_rules!|class|def|async def|function|interface|func|fun|val|var|let|record|object|data class|final class|abstract class|protocol|module|package";
const PREFIX: &str = r"^[[:space:]]*(export[[:space:]]+(default[[:space:]]+)?)?(pub(\([a-z]+\))?[[:space:]]+)?(async[[:space:]]+)?(unsafe[[:space:]]+)?(static[[:space:]]+)?(public|private|protected|internal|open|final|override|abstract)?[[:space:]]*";
const NAME: &str = "[A-Za-z_][A-Za-z0-9_]*";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Located {
    pub symbol: Option<String>,
    pub path: Option<String>,
    pub definitions: Vec<Definition>,
    pub files_named: Vec<String>,
    pub definitions_truncated: bool,
}

pub fn pattern(symbol: Option<&str>) -> String {
    let name = symbol.map_or(NAME.to_string(), regex::escape);
    format!("{PREFIX}({KEYWORDS})[[:space:]]+{name}([^A-Za-z0-9_]|$)")
}

pub async fn locate(repo: &str, symbol: Option<&str>, path: Option<&str>) -> Located {
    let pattern = pattern(symbol);
    let mut args = vec![
        "grep",
        "-n",
        "-I",
        "-E",
        "--untracked",
        "-e",
        pattern.as_str(),
        "--",
    ];
    if let Some(p) = path {
        args.push(p);
    }
    let raw = git(repo, &args, None).await.unwrap_or_default();
    let (definitions, definitions_truncated) = parse(&String::from_utf8_lossy(&raw));
    let files_named = match (symbol, path) {
        (Some(name), None) => files_named(repo, name).await,
        _ => Vec::new(),
    };
    Located {
        symbol: symbol.map(str::to_string),
        path: path.map(str::to_string),
        definitions,
        files_named,
        definitions_truncated,
    }
}

pub fn parse(raw: &str) -> (Vec<Definition>, bool) {
    let mut out = Vec::new();
    for line in raw.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(path), Some(number), Some(text)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(line) = number.parse() else { continue };
        out.push(Definition {
            path: path.to_string(),
            line,
            text: text.trim().to_string(),
        });
    }
    let truncated = out.len() > MOST;
    out.truncate(MOST);
    (out, truncated)
}

async fn files_named(repo: &str, name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let raw = git(
        repo,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
        None,
    )
    .await
    .unwrap_or_default();
    String::from_utf8_lossy(&raw)
        .lines()
        .filter(|p| {
            std::path::Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.to_lowercase() == lower)
        })
        .take(MOST)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pattern_finds_a_definition_in_each_language_and_not_a_use() {
        let re = regex::Regex::new(&pattern(Some("load_rules")).replace("[[:space:]]", r"\s"))
            .unwrap_or_else(|e| panic!("{e}"));
        for line in [
            "pub fn load_rules(path: &str) {",
            "    pub async fn load_rules() {",
            "def load_rules(path):",
            "export async function load_rules() {",
            "export default class load_rules {",
            "func load_rules() error {",
            "fun load_rules(): Unit",
        ] {
            assert!(re.is_match(line), "{line}");
        }
        for line in [
            "let rules = load_rules(path);",
            "fn load_rules_from(path: &str) {",
            "// fn load_rules was here",
        ] {
            assert!(!re.is_match(line), "{line}");
        }
    }

    #[test]
    fn grep_output_becomes_definitions_with_a_cap() {
        let (found, truncated) =
            parse("src/a.rs:12:pub fn x() {}\nbad line\nsrc/b.py:3:def y():\n");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].path, "src/b.py");
        assert_eq!(found[1].line, 3);
        assert_eq!(found[1].text, "def y():");
        assert!(!truncated);
    }
}
