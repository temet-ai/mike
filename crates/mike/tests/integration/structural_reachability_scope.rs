use mike::steerer::structural::{SourceFile, build_import_graph, check_reachability};

fn file(path: &str, content: &str) -> SourceFile {
    SourceFile {
        path: path.to_owned(),
        content: content.to_owned(),
    }
}

#[test]
fn reachability_walk_is_global_not_scoped_to_the_surface_prefix() {
    let files = vec![
        file(
            "frontend/src/routes/unrelated/+page.svelte",
            "<script>import Orphan from '$lib/comms/Orphan.svelte';</script>",
        ),
        file("frontend/src/lib/comms/Orphan.svelte", "<div>orphan</div>"),
    ];

    let graph = build_import_graph(&files);
    let findings = check_reachability(&files, &graph, "frontend/src/lib/comms/");

    assert!(
        findings.is_empty(),
        "expected check_reachability to report Orphan.svelte as reachable via the unrelated \
         route's import — this is the check's confirmed, deliberately-unfixed behavior per #185 \
         — but got unreachable findings instead: {:?}. If this fails, the walk's scoping has \
         changed since #185 was closed and #156/#185 need re-reading.",
        findings.iter().map(|f| &f.file).collect::<Vec<_>>()
    );
}

#[test]
fn reachability_walk_correctly_flags_a_genuinely_unimported_file() {
    let files = vec![file(
        "frontend/src/lib/comms/Orphan.svelte",
        "<div>orphan</div>",
    )];

    let graph = build_import_graph(&files);
    let findings = check_reachability(&files, &graph, "frontend/src/lib/comms/");

    assert_eq!(
        findings.iter().map(|f| f.file.as_str()).collect::<Vec<_>>(),
        vec!["frontend/src/lib/comms/Orphan.svelte"],
        "a file imported by nothing at all must still be flagged unreachable"
    );
}
