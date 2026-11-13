use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::checks::normal;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    pub text: String,
}

const SHAPES: [&str; 4] = [
    r"^\s*-->\s*(?P<path>[^\s:]+):(?P<line>\d+)",
    r#"File "(?P<path>[^"]+)", line (?P<line>\d+)"#,
    r"(?P<path>[A-Za-z0-9_@./\\-]+\.[A-Za-z0-9]+)\((?P<line>\d+),\d+\)",
    r"(?P<path>[A-Za-z0-9_@./\\-]+\.[A-Za-z0-9]+):(?P<line>\d+)(?::\d+)?\b",
];

fn shapes() -> &'static [Regex] {
    static SHAPES_COMPILED: OnceLock<Vec<Regex>> = OnceLock::new();
    SHAPES_COMPILED.get_or_init(|| SHAPES.iter().filter_map(|s| Regex::new(s).ok()).collect())
}

pub fn parse(output: &str, repo: &str) -> Vec<Diagnostic> {
    let mut found = BTreeSet::new();
    for line in output.lines() {
        if let Some(found_here) = first_match(line, repo) {
            found.insert(found_here);
        }
    }
    found.into_iter().collect()
}

fn first_match(line: &str, repo: &str) -> Option<Diagnostic> {
    shapes().iter().find_map(|shape| {
        let caps = shape.captures(line)?;
        let path = relative(caps.name("path")?.as_str(), repo)?;
        let number = caps.name("line")?.as_str().parse().ok()?;
        Some(Diagnostic {
            path,
            line: number,
            text: line.trim().to_string(),
        })
    })
}

fn relative(path: &str, repo: &str) -> Option<String> {
    let trimmed = path.trim_start_matches("./");
    let inside = trimmed
        .strip_prefix(repo)
        .map(|rest| rest.trim_start_matches('/'))
        .unwrap_or(trimmed);
    if inside.starts_with('/') || inside.is_empty() || !inside.contains('.') {
        return None;
    }
    Some(normal(inside))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn every_compiler_shape_names_its_file_and_line() {
        let output = "\
error[E0425]: cannot find value `x`
  --> crates/mike/src/lib.rs:12:5
src/app.ts(7,3): error TS2304: Cannot find name 'y'.
  File \"/repo/pkg/mod.py\", line 3, in <module>
main.go:9:2: undefined: z
warning: unused import
   --> crates/mike/src/lib.rs:12:5
Finished in 1.2s";
        let found = parse(output, "/repo");
        let seen: Vec<(String, u32)> = found.iter().map(|d| (d.path.clone(), d.line)).collect();
        assert_eq!(
            seen,
            vec![
                ("crates/mike/src/lib.rs".into(), 12),
                ("main.go".into(), 9),
                ("pkg/mod.py".into(), 3),
                ("src/app.ts".into(), 7),
            ]
        );
    }

    #[test]
    fn a_line_without_a_file_is_not_a_diagnostic() {
        assert!(
            parse(
                "Compiling mike v0.1.0\nerror: aborting due to 2 errors",
                "/r"
            )
            .is_empty()
        );
        assert!(parse("at 12:30:00 the build ran", "/r").is_empty());
    }
}
