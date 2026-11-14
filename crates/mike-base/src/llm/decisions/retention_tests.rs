use std::path::{Path, PathBuf};

fn workspace_crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            source_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "sql") {
            out.push(path);
        }
    }
}

fn prunes_decisions(text: &str) -> bool {
    let flat = text
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        "delete from decisions.",
        "truncate decisions.",
        "truncate table decisions.",
        "drop table decisions.",
        "drop schema decisions",
    ]
    .iter()
    .any(|needle| flat.contains(needle))
}

#[test]
fn no_code_or_migration_deletes_from_the_decisions_schema() {
    let mut files = Vec::new();
    source_files(&workspace_crates(), &mut files);
    assert!(files.len() > 100, "the scan found the workspace sources");
    let this_file = Path::new(file!()).file_name().map(ToOwned::to_owned);
    let offenders: Vec<_> = files
        .iter()
        .filter(|p| p.file_name().map(ToOwned::to_owned) != this_file)
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|t| prunes_decisions(&t)))
        .collect();
    assert!(
        offenders.is_empty(),
        "decisions.records and decisions.outcomes are never pruned, but these files delete from them: {offenders:?}"
    );
}

#[test]
fn the_scan_recognises_a_prune() {
    assert!(prunes_decisions(
        "DELETE FROM\n  decisions.records WHERE id = 1"
    ));
    assert!(prunes_decisions("truncate decisions.outcomes"));
    assert!(!prunes_decisions("SELECT * FROM decisions.records"));
}
