use std::path::{Path, PathBuf};

const PROJECTION_WRITER: &str = "store/persistence/operator_turns/projection.sql";

const R3_PATH_WRITERS: [&str; 2] = [
    "store/persistence/operator_turns/judgment.rs",
    "store/persistence/turn_verdicts/insert_live.sql",
];

use crate::split_crates::src_roots;

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let skipped_dir = matches!(name.as_str(), "migrations" | "test_support" | "tests");
        let test_file =
            name.ends_with("_tests.rs") || name.ends_with("_test.rs") || name == "tests.rs";
        if path.is_dir() && !skipped_dir {
            source_files(&path, out)?;
        } else if !test_file && path.extension().is_some_and(|e| e == "rs" || e == "sql") {
            out.push(path);
        }
    }
    Ok(())
}

fn statement_after<'a>(text: &'a str, start: usize, ends: &[&str]) -> &'a str {
    let rest = &text[start..];
    let end = ends
        .iter()
        .filter_map(|e| rest[1..].find(e).map(|i| i + 1))
        .min()
        .unwrap_or(rest.len());
    &rest[..end]
}

fn writes_outcome_id(text: &str) -> usize {
    let updates = text
        .match_indices("UPDATE memory.operator_turns")
        .filter(|(i, _)| statement_after(text, *i, &["WHERE", "RETURNING"]).contains("outcome_id"))
        .count();
    let inserts = text
        .match_indices("INSERT INTO memory.operator_turns")
        .filter(|(i, _)| statement_after(text, *i, &["VALUES", "SELECT"]).contains("outcome_id"))
        .count();
    updates + inserts
}

#[test]
fn outcome_id_has_exactly_one_writer_outside_the_r3_path() -> anyhow::Result<()> {
    let mut writers = Vec::new();
    for root in src_roots()? {
        let mut files = Vec::new();
        source_files(&root, &mut files)?;
        for file in files {
            let text = std::fs::read_to_string(&file)?;
            let n = writes_outcome_id(&text);
            if n > 0 {
                let rel = file
                    .strip_prefix(&root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                writers.push((rel, n));
            }
        }
    }
    let projection: Vec<_> = writers
        .iter()
        .filter(|(f, _)| f == PROJECTION_WRITER)
        .collect();
    assert_eq!(
        projection.len(),
        1,
        "the projection must write outcome_id: {writers:?}"
    );
    assert_eq!(
        projection[0].1, 1,
        "projection.sql holds one outcome_id write"
    );
    let others: Vec<_> = writers
        .iter()
        .filter(|(f, _)| f != PROJECTION_WRITER && !R3_PATH_WRITERS.contains(&f.as_str()))
        .collect();
    assert!(
        others.is_empty(),
        "outcome_id has a writer outside the projection: {others:?}"
    );
    Ok(())
}

#[test]
fn the_writer_scan_sees_an_update_and_ignores_a_reader() {
    assert_eq!(
        writes_outcome_id("UPDATE memory.operator_turns t SET outcome_id = 1 WHERE t.id = 2"),
        1
    );
    assert_eq!(
        writes_outcome_id(
            "UPDATE memory.operator_turns t SET closed_at = now() WHERE t.outcome_id = 1"
        ),
        0
    );
    assert_eq!(
        writes_outcome_id("SELECT outcome_id FROM memory.operator_turns"),
        0
    );
}
