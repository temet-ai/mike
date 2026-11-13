use std::path::{Path, PathBuf};

const DELIVERY_MARKERS: [&str; 6] = [
    "record_and_broadcast",
    "insert_intervention",
    "steering.interventions",
    "remote_control.injections",
    "push_reminder",
    "SystemReminder",
];

fn guard_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            guard_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn guard_code_has_no_delivery_path() -> anyhow::Result<()> {
    let mut files = Vec::new();
    for src in crate::split_crates::src_roots()? {
        let root = src.join("steerer/guard");
        if root.is_dir() {
            guard_files(&root, &mut files)?;
        }
    }
    assert!(
        !files.is_empty(),
        "no guard sources under any crate's src/steerer/guard"
    );
    let mut hits = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        for marker in DELIVERY_MARKERS {
            if text.contains(marker) {
                hits.push(format!("{}: {marker}", file.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the JC16 guard must not deliver: {hits:#?}"
    );
    Ok(())
}
