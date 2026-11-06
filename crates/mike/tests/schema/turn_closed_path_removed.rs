use std::path::{Path, PathBuf};

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn files_containing(needle: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    rust_files(&src_root(), &mut files)?;
    let mut hits = Vec::new();
    for file in files {
        if std::fs::read_to_string(&file)?.contains(needle) {
            hits.push(file);
        }
    }
    Ok(hits)
}

#[test]
fn turn_closed_trigger_path_and_fast_path_are_deleted() -> anyhow::Result<()> {
    assert!(
        !src_root()
            .join("steerer/fleet/triggers/turn_close")
            .exists(),
        "the turn_closed trigger module must stay deleted"
    );
    for needle in [
        "evaluate_turn_close_triggers",
        "mark_turn_corrected_fast_path",
        "fn mark_turn_corrected",
    ] {
        let hits = files_containing(needle)?;
        assert!(hits.is_empty(), "{needle} is back in {hits:?}");
    }
    let run_loop = std::fs::read_to_string(src_root().join("steerer/fleet/run_loop.rs"))?;
    assert!(
        !run_loop.contains("ProxyEvent::OperatorTurnClosed"),
        "the steerer fleet loop must not handle OperatorTurnClosed"
    );
    Ok(())
}

#[test]
fn the_llm_correction_classifier_is_deleted() -> anyhow::Result<()> {
    for needle in [
        "correction_system_prompt",
        "maybe_spawn_correction_classifier",
        "is_plausibly_corrective",
        "\"steerer.correction.",
    ] {
        let hits = files_containing(needle)?;
        assert!(hits.is_empty(), "{needle} is back in {hits:?}");
    }
    assert!(!src_root().join("analysis/correction.rs").exists());
    Ok(())
}
