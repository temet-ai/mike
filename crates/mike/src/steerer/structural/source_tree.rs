use std::path::Path;

pub struct SourceFile {
    pub path: String,
    pub content: String,
}

pub fn read_source_tree(repo_root: &Path) -> Vec<SourceFile> {
    let mut out = Vec::new();
    walk(&repo_root.join("frontend/src"), repo_root, &mut out);
    out
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".svelte-kit",
    "build",
    "dist",
    "__snapshots__",
    "__screenshots__",
];

fn walk(dir: &Path, repo_root: &Path, out: &mut Vec<SourceFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| SKIP_DIRS.contains(&n))
            {
                continue;
            }
            walk(&path, repo_root, out);
            continue;
        }
        let is_source = path.extension().is_some_and(|e| e == "svelte" || e == "ts");
        if !is_source {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(rel) = path.strip_prefix(repo_root) else {
            continue;
        };
        out.push(SourceFile {
            path: rel.to_string_lossy().replace('\\', "/"),
            content,
        });
    }
}
