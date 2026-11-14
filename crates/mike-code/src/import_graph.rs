use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::source_tree::SourceFile;

static IMPORT_SPEC_RE: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r#"(?:from|import)\s+['"]([^'"]+)['"]"#) {
            Ok(re) => re,
            Err(e) => panic!("invalid static import-spec pattern: {e}"),
        },
    );

pub struct ImportGraph {
    pub edges: HashMap<String, Vec<String>>,
}

pub fn build_import_graph(files: &[SourceFile]) -> ImportGraph {
    let by_path: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut edges = HashMap::with_capacity(files.len());
    for f in files {
        let importer_dir = Path::new(&f.path).parent().unwrap_or_else(|| Path::new(""));
        let mut targets = Vec::new();
        for cap in IMPORT_SPEC_RE.captures_iter(&f.content) {
            let spec = &cap[1];
            if let Some(candidate) = candidate_path(spec, importer_dir)
                && let Some(resolved) = resolve_against_known_files(&candidate, &by_path)
            {
                targets.push(resolved);
            }
        }
        edges.insert(f.path.clone(), targets);
    }
    ImportGraph { edges }
}

fn candidate_path(spec: &str, importer_dir: &Path) -> Option<PathBuf> {
    let raw = if let Some(rest) = spec.strip_prefix("$lib/") {
        Path::new("frontend/src/lib").join(rest)
    } else if spec.starts_with("./") || spec.starts_with("../") {
        importer_dir.join(spec)
    } else {
        return None;
    };
    Some(lexical_normalize(&raw))
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut stack: Vec<std::path::Component> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                stack.pop();
            }
            std::path::Component::CurDir => {}
            other => stack.push(other),
        }
    }
    stack.into_iter().collect()
}

const RESOLVE_EXTENSIONS: &[&str] = &[".ts", ".svelte", ".js"];

fn resolve_against_known_files(candidate: &Path, by_path: &HashSet<&str>) -> Option<String> {
    let candidate_str = candidate.to_string_lossy().replace('\\', "/");
    if by_path.contains(candidate_str.as_str()) {
        return Some(candidate_str);
    }
    for ext in RESOLVE_EXTENSIONS {
        let with_ext = format!("{candidate_str}{ext}");
        if by_path.contains(with_ext.as_str()) {
            return Some(with_ext);
        }
    }
    for ext in RESOLVE_EXTENSIONS {
        let index = format!("{candidate_str}/index{ext}");
        if by_path.contains(index.as_str()) {
            return Some(index);
        }
    }
    None
}
