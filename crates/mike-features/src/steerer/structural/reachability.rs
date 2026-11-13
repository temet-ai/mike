use std::collections::HashSet;

use super::import_graph::ImportGraph;
use super::source_tree::SourceFile;

pub struct UnreachableFinding {
    pub file: String,
}

pub fn check_reachability(
    files: &[SourceFile],
    graph: &ImportGraph,
    surface_prefix: &str,
) -> Vec<UnreachableFinding> {
    let route_entries: Vec<&str> = files
        .iter()
        .filter(|f| {
            f.path.starts_with("frontend/src/routes/")
                && (f.path.ends_with("/+page.svelte") || f.path.ends_with("/+layout.svelte"))
        })
        .map(|f| f.path.as_str())
        .collect();

    let mut reachable: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = route_entries;
    while let Some(cur) = stack.pop() {
        if !reachable.insert(cur) {
            continue;
        }
        if let Some(targets) = graph.edges.get(cur) {
            for t in targets {
                if !reachable.contains(t.as_str()) {
                    stack.push(t.as_str());
                }
            }
        }
    }

    files
        .iter()
        .filter(|f| f.path.starts_with(surface_prefix))
        .filter(|f| !is_test_or_story_artifact(&f.path))
        .filter(|f| !reachable.contains(f.path.as_str()))
        .map(|f| UnreachableFinding {
            file: f.path.clone(),
        })
        .collect()
}

fn is_test_or_story_artifact(path: &str) -> bool {
    path.ends_with(".test.ts")
        || path.ends_with(".spec.ts")
        || path.ends_with(".stories.svelte")
        || path.contains("__harness__")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::import_graph::build_import_graph;
    use super::super::source_tree::read_source_tree;
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn reachability_check_flags_a_component_no_route_imports() {
        let files = vec![
            file(
                "frontend/src/routes/comms/+page.svelte",
                "<script>import Used from '$lib/comms/Used.svelte';</script>",
            ),
            file("frontend/src/lib/comms/Used.svelte", "<div>used</div>"),
            file("frontend/src/lib/comms/Orphan.svelte", "<div>orphan</div>"),
        ];
        let graph = build_import_graph(&files);
        let findings = check_reachability(&files, &graph, "frontend/src/lib/comms/");
        let orphans: Vec<&str> = findings.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(orphans, vec!["frontend/src/lib/comms/Orphan.svelte"]);
    }

    #[test]
    fn reachability_check_passes_when_every_surface_file_is_imported_transitively() {
        let files = vec![
            file(
                "frontend/src/routes/comms/+page.svelte",
                "<script>import A from '$lib/comms/A.svelte';</script>",
            ),
            file(
                "frontend/src/lib/comms/A.svelte",
                "<script>import B from './B.svelte';</script>",
            ),
            file("frontend/src/lib/comms/B.svelte", "<div>b</div>"),
        ];
        let graph = build_import_graph(&files);
        assert!(check_reachability(&files, &graph, "frontend/src/lib/comms/").is_empty());
    }

    #[test]
    fn live_reachability_check_passes_the_current_comms_surface() {
        let files = read_source_tree(&repo_root());
        let graph = build_import_graph(&files);
        let findings = check_reachability(&files, &graph, "frontend/src/lib/comms/");
        assert!(
            findings.is_empty(),
            "expected every comms component to be reachable (found unreachable: {:?})",
            findings.iter().map(|f| &f.file).collect::<Vec<_>>()
        );
    }
}
