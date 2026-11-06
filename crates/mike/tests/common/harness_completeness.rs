use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;

const CONCERNS: [&str; 5] = ["proxy", "integration", "e2e", "cli", "schema"];

#[test]
fn concern_harnesses_reference_every_source_exactly_once() -> anyhow::Result<()> {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");

    for concern in CONCERNS {
        let harness_path = tests_dir.join(format!("{concern}.rs"));
        let harness = fs::read_to_string(&harness_path)
            .with_context(|| format!("failed to read {}", harness_path.display()))?;
        let declared = declared_paths(&harness);
        let concern_prefix = format!("{concern}/");

        for declared_path in &declared {
            let resolved = tests_dir.join(declared_path);
            anyhow::ensure!(
                resolved.is_file(),
                "{} declares missing path {declared_path}",
                harness_path.display()
            );

            let belongs_to_a_concern = CONCERNS
                .iter()
                .any(|candidate| declared_path.starts_with(&format!("{candidate}/")));
            anyhow::ensure!(
                !belongs_to_a_concern || declared_path.starts_with(&concern_prefix),
                "{} declares concern path {declared_path} outside {concern}/",
                harness_path.display()
            );
        }

        let mut declaration_counts = BTreeMap::<String, usize>::new();
        for declared_path in declared
            .iter()
            .filter(|path| path.starts_with(&concern_prefix))
        {
            *declaration_counts
                .entry((*declared_path).to_owned())
                .or_default() += 1;
        }

        let concern_dir = tests_dir.join(concern);
        let entries = fs::read_dir(&concern_dir)
            .with_context(|| format!("failed to read {}", concern_dir.display()))?;
        for entry in entries {
            let entry =
                entry.with_context(|| format!("failed to read {}", concern_dir.display()))?;
            let path = entry.path();
            let Some(relative_path) = declarable_unit(concern, &path, entry.file_type()?)? else {
                continue;
            };

            let count = declaration_counts
                .remove(&relative_path)
                .unwrap_or_default();
            anyhow::ensure!(
                count == 1,
                "{relative_path} must be declared exactly once in {}, found {count}",
                harness_path.display()
            );
        }

        anyhow::ensure!(
            declaration_counts.is_empty(),
            "{} contains stale concern declarations: {:?}",
            harness_path.display(),
            declaration_counts
        );
    }

    Ok(())
}

fn declarable_unit(
    concern: &str,
    path: &Path,
    file_type: fs::FileType,
) -> anyhow::Result<Option<String>> {
    if file_type.is_file() {
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            return Ok(None);
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("non-UTF-8 test source path {}", path.display()))?;
        return Ok(Some(format!("{concern}/{file_name}")));
    }

    if file_type.is_dir() {
        if !path.join("mod.rs").is_file() {
            return Ok(None);
        }
        let dir_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("non-UTF-8 test source path {}", path.display()))?;
        return Ok(Some(format!("{concern}/{dir_name}/mod.rs")));
    }

    Ok(None)
}

fn declared_paths(harness: &str) -> Vec<&str> {
    harness
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("#[path = \"")
                .and_then(|path| path.strip_suffix("\"]"))
        })
        .collect()
}
