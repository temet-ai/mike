use std::path::{Path, PathBuf};

const JUDGE_INPUT_DIRS: [&str; 3] = [
    "src/analysis/judge",
    "src/judge_worker",
    "src/dossier/render",
];
const MARKERS: [&str; 4] = [
    "calibration_block",
    "cross_session",
    "cal-v1",
    "CALIBRATION_PARAGRAPH",
];

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e == "rs" || e == "txt" || e == "md")
        {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn turn_and_session_judge_inputs_carry_no_calibration_line() -> anyhow::Result<()> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut all = Vec::new();
    for name in ["mike", "mike-base", "mike-features"] {
        for dir in JUDGE_INPUT_DIRS {
            files(&crates.join(name).join(dir), &mut all)?;
        }
    }
    assert!(!all.is_empty());
    let mut hits = Vec::new();
    for file in all {
        let text = std::fs::read_to_string(&file)?;
        for marker in MARKERS {
            if text.contains(marker) {
                hits.push(format!("{}: {marker}", file.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the calibration block belongs to the turn guard only: {hits:#?}"
    );
    Ok(())
}
