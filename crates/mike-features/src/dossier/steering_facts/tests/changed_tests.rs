use chrono::Utc;
use serde_json::json;

use crate::dossier::steering_facts::changed::{
    ChangedPath, PathKind, changed_paths, mark_committed, path_kind, summary,
};
use crate::dossier::steering_facts::observation::{CallFact, observe};

fn call(id: i64, tool: &str, input: serde_json::Value) -> CallFact {
    CallFact {
        use_id: id,
        at: Utc::now(),
        tool_name: tool.to_owned(),
        input,
        has_result: true,
    }
}

#[test]
fn path_kind_reads_extension_and_conventional_directories() {
    let cases = [
        ("crates/mike/src/lib.rs", PathKind::Code),
        ("crates/mike/tests/integration/x.rs", PathKind::Test),
        ("frontend/src/lib/a.test.ts", PathKind::Test),
        ("frontend/src/lib/Card.svelte", PathKind::Ui),
        ("frontend/src/app.css", PathKind::Style),
        ("docs/ARCHITECTURE.md", PathKind::Doc),
        ("Cargo.toml", PathKind::Config),
        (".github/workflows/x.yml", PathKind::Config),
        ("Cargo.lock", PathKind::Generated),
        ("frontend/dist/app.js", PathKind::Generated),
        ("assets/logo.png", PathKind::Other),
    ];
    for (path, kind) in cases {
        assert_eq!(path_kind(path), kind, "{path}");
    }
}

#[test]
fn changed_paths_counts_lines_per_path_newest_first() {
    let window = vec![
        call(
            1,
            "Edit",
            json!({"file_path": "/r/a.rs", "old_string": "x\ny", "new_string": "x\ny\nz"}),
        ),
        call(
            2,
            "Write",
            json!({"file_path": "/r/b.md", "content": "one\ntwo"}),
        ),
        call(
            3,
            "MultiEdit",
            json!({"file_path": "/r/a.rs", "edits": [
                {"old_string": "p", "new_string": "q\nr"}
            ]}),
        ),
    ];
    let changed = changed_paths(&window);
    assert_eq!(changed.len(), 2);
    assert_eq!(changed[0].path, "/r/a.rs");
    assert_eq!((changed[0].added, changed[0].removed), (5, 3));
    assert_eq!(changed[1].path, "/r/b.md");
    assert_eq!((changed[1].added, changed[1].removed), (2, 0));
    assert_eq!(changed[1].kind, PathKind::Doc);
}

#[test]
fn observe_lists_only_paths_written_since_the_last_check() {
    let calls = vec![
        call(
            1,
            "Edit",
            json!({"file_path": "/r/old.rs", "old_string": "a", "new_string": "b"}),
        ),
        call(2, "Bash", json!({"command": "cargo test"})),
        call(
            3,
            "Edit",
            json!({"file_path": "/r/new.rs", "old_string": "a", "new_string": "b"}),
        ),
    ];
    let o = observe(&calls);
    let paths: Vec<&str> = o.changed.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, vec!["/r/new.rs"]);
}

#[test]
fn mark_committed_makes_paths_relative_and_reads_the_open_set() {
    let mut changed = vec![
        ChangedPath {
            path: "/repo/src/a.rs".to_owned(),
            kind: PathKind::Code,
            added: 1,
            removed: 0,
            committed: None,
        },
        ChangedPath {
            path: "/repo/src/b.rs".to_owned(),
            kind: PathKind::Code,
            added: 1,
            removed: 0,
            committed: None,
        },
        ChangedPath {
            path: "/elsewhere/c.rs".to_owned(),
            kind: PathKind::Code,
            added: 1,
            removed: 0,
            committed: None,
        },
    ];
    mark_committed(&mut changed, "/repo/", &["src/a.rs".to_owned()]);
    assert_eq!(changed[0].path, "src/a.rs");
    assert_eq!(changed[0].committed, Some(false));
    assert_eq!(changed[1].committed, Some(true));
    assert_eq!(changed[2].committed, None);
}

#[test]
fn an_empty_open_set_means_everything_is_committed() {
    let mut changed = vec![ChangedPath {
        path: "/repo/src/a.rs".to_owned(),
        kind: PathKind::Code,
        added: 1,
        removed: 0,
        committed: None,
    }];
    mark_committed(&mut changed, "/repo", &[]);
    assert_eq!(changed[0].committed, Some(true));
}

#[test]
fn summary_names_the_newest_file_and_counts_the_rest() {
    let one = ChangedPath {
        path: "/Users/x/projects/repo/frontend/src/Card.svelte".to_owned(),
        kind: PathKind::Ui,
        added: 12,
        removed: 3,
        committed: Some(false),
    };
    assert_eq!(
        summary(std::slice::from_ref(&one)).as_deref(),
        Some("repo/frontend/src/Card.svelte (UI, +12 \u{2212}3, uncommitted)")
    );
    let two = vec![one.clone(), one.clone(), one];
    assert!(summary(&two).is_some_and(|s| s.ends_with("and 2 more")));
    assert_eq!(summary(&[]), None);
}
