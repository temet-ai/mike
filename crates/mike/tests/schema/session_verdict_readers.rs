use std::path::{Path, PathBuf};

const TABLE: &str = "session_judges.verdicts";

const WINDOW: usize = 400;

struct NotLiveRead {
    file: &'static str,
    scope: &'static str,
    reason: &'static str,
}

const NOT_LIVE_READS: [NotLiveRead; 3] = [
    NotLiveRead {
        file: "store/persistence/session_end/history_all.sql",
        scope: "v.session_id = $1",
        reason: "GET /api/sessions/{id}/verdicts with no mode is the documented history of \
                 every mode for one session, and each row carries its mode",
    },
    NotLiveRead {
        file: "store/persistence/session_end/refresh_run.sql",
        scope: "v.mode = 'eval'",
        reason: "totals the cost of an eval run from that run's own eval verdicts",
    },
    NotLiveRead {
        file: "store/persistence/session_end/shadow_candidates.sql",
        scope: "v.mode = 'shadow'",
        reason: "skips end turns the shadow judge has already judged under this bundle",
    },
];

fn src_roots() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    ["mike", "mike-base", "mike-features"]
        .iter()
        .map(|name| crates.join(name).join("src"))
        .collect()
}

fn locate(relative: &str) -> PathBuf {
    let roots = src_roots();
    roots
        .iter()
        .map(|root| root.join(relative))
        .find(|path| path.exists())
        .unwrap_or_else(|| roots[0].join(relative))
}

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

fn is_write(before: &str) -> bool {
    let head = before.trim_end().to_ascii_uppercase();
    head.ends_with("INSERT INTO") || head.ends_with("UPDATE") || head.ends_with("DELETE FROM")
}

fn window_after(text: &str, start: usize) -> &str {
    let mut end = (start + WINDOW).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[start..end]
}

fn unfiltered_reads(text: &str) -> Vec<usize> {
    text.match_indices(TABLE)
        .filter(|(i, _)| {
            let after = &text[i + TABLE.len()..];
            let label_literal = after.starts_with('"') && text[..*i].ends_with('"');
            let before_start = i.saturating_sub(20);
            let before = &text[before_start..*i];
            !label_literal && !is_write(before)
        })
        .filter(|(i, _)| {
            let window = window_after(text, *i);
            !(window.contains("mode = 'live'") || window.contains("mode = $"))
        })
        .map(|(i, _)| text[..i].lines().count())
        .collect()
}

fn offending_reads(rel: &str, text: &str) -> Vec<usize> {
    let reads = unfiltered_reads(text);
    let listed = NOT_LIVE_READS.iter().find(|read| read.file == rel);
    if listed.is_some_and(|read| reads.len() == 1 && text.contains(read.scope)) {
        return Vec::new();
    }
    reads
}

#[test]
fn session_verdict_readers_every_read_filters_live_mode() -> anyhow::Result<()> {
    let mut offenders = Vec::new();
    for root in src_roots() {
        let mut files = Vec::new();
        source_files(&root, &mut files)?;
        for file in files {
            let text = std::fs::read_to_string(&file)?;
            let rel = file
                .strip_prefix(&root)?
                .to_string_lossy()
                .replace('\\', "/");
            for line in offending_reads(&rel, &text) {
                offenders.push(format!("{rel}:{line}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "readers of {TABLE} without a mode = 'live' filter: {offenders:#?}"
    );
    Ok(())
}

#[test]
fn session_verdict_readers_every_listed_read_is_still_that_read() -> anyhow::Result<()> {
    for read in &NOT_LIVE_READS {
        let text = std::fs::read_to_string(locate(read.file))?;
        assert_eq!(
            unfiltered_reads(&text).len(),
            1,
            "{} must hold exactly the one read listed for it: {}",
            read.file,
            read.reason
        );
        assert!(
            text.contains(read.scope),
            "{} no longer scopes its read with {}: {}",
            read.file,
            read.scope,
            read.reason
        );
    }
    Ok(())
}

#[test]
fn session_verdict_readers_a_second_read_in_a_listed_file_is_flagged() {
    let rel = "store/persistence/session_end/refresh_run.sql";
    let one = "SELECT 1 FROM session_judges.verdicts v WHERE v.mode = 'eval'";
    let two = "SELECT 1 FROM session_judges.verdicts v WHERE v.mode = 'eval';
               SELECT 2 FROM session_judges.verdicts w WHERE w.id = 1";
    let unscoped = "SELECT 1 FROM session_judges.verdicts v WHERE v.id = 1";
    assert!(offending_reads(rel, one).is_empty());
    assert_eq!(offending_reads(rel, two).len(), 2);
    assert_eq!(offending_reads(rel, unscoped).len(), 1);
    assert_eq!(offending_reads("other.sql", one).len(), 1);
}

#[test]
fn session_verdict_readers_scanner_flags_an_unfiltered_read() {
    let unfiltered = "SELECT v.id FROM session_judges.verdicts v WHERE v.session_id = $1";
    let filtered = "SELECT v.id FROM session_judges.verdicts v WHERE v.mode = 'live'";
    let insert = "INSERT INTO session_judges.verdicts (session_id) VALUES ($1)";
    assert_eq!(unfiltered_reads(unfiltered).len(), 1);
    assert!(unfiltered_reads(filtered).is_empty());
    assert!(unfiltered_reads(insert).is_empty());
}
