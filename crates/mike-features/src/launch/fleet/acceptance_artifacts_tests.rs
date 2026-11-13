use std::os::unix::fs::symlink;

use super::*;

const RUN: &str = "fleet-project-a1234567";

fn evidence_folder(root: &Path, run: &str) -> PathBuf {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .join("fleet-runs")
        .join(run)
        .join("acceptance")
}

fn names(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

#[test]
fn prepare_creates_the_evidence_folder_with_both_subfolders_and_no_manifest() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let folder = prepare(root.path(), RUN)?;
    assert_eq!(folder, evidence_folder(root.path(), RUN));
    assert_eq!(names(&folder)?, ["artifacts", "comparisons"]);
    assert!(folder.join("comparisons").is_dir());
    assert!(folder.join("artifacts").is_dir());
    assert!(!folder.join("manifest.json").exists());
    Ok(())
}

#[test]
fn successor_reuses_the_folder_and_leaves_existing_evidence_untouched() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let folder = prepare(root.path(), RUN)?;
    let shot = folder.join("artifacts/home.png");
    let log = folder.join("comparisons/live.log");
    let payload = "live observation\n".repeat(10_000);
    std::fs::write(&shot, [0x89, b'P', b'N', b'G'])?;
    std::fs::write(&log, &payload)?;
    let successor = prepare(root.path(), RUN)?;
    assert_eq!(successor, folder);
    assert_eq!(std::fs::read(&shot)?, [0x89, b'P', b'N', b'G']);
    assert_eq!(std::fs::read_to_string(&log)?, payload);
    assert_eq!(names(&folder)?, ["artifacts", "comparisons"]);
    Ok(())
}

#[test]
fn different_runs_never_share_an_evidence_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let first = prepare(root.path(), RUN)?;
    let second = prepare(root.path(), "fleet-project-b1234567")?;
    assert_ne!(first, second);
    std::fs::write(first.join("artifacts/a.png"), "a")?;
    assert!(names(&second.join("artifacts"))?.is_empty());
    Ok(())
}

#[test]
fn concurrent_launches_all_get_the_same_complete_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| prepare(root.path(), RUN)))
            .collect();
        for thread in threads {
            let folder = thread
                .join()
                .map_err(|_| anyhow::anyhow!("launch panicked"))??;
            assert_eq!(folder, evidence_folder(root.path(), RUN));
        }
        Ok::<_, anyhow::Error>(())
    })?;
    let folder = evidence_folder(root.path(), RUN);
    assert_eq!(names(&folder)?, ["artifacts", "comparisons"]);
    Ok(())
}

#[test]
fn unsafe_run_and_relative_storage_are_refused_before_creating_anything() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for run in [
        "fleet-../escape",
        "fleet-x/y",
        "fleet-x\\y",
        "fleet-",
        "../fleet-x",
        "fleet-x\n",
    ] {
        assert!(prepare(root.path(), run).is_err(), "{run}");
    }
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    assert!(prepare(Path::new("relative"), RUN).is_err());
    assert!(prepare(&root.path().join("../escape"), RUN).is_err());
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[test]
fn symlinked_storage_components_and_file_collisions_fail_safely() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let path = root.path().join("fleet-runs");
    symlink(outside.path(), &path)?;
    assert!(prepare(root.path(), RUN).is_err());
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    std::fs::remove_file(&path)?;
    std::fs::write(&path, "keep")?;
    assert!(prepare(root.path(), RUN).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "keep");
    Ok(())
}

#[test]
fn a_file_or_symlink_in_the_evidence_folders_place_is_refused() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let folder = prepare(root.path(), RUN)?;
    let subfolder = folder.join("artifacts");
    std::fs::remove_dir(&subfolder)?;
    std::fs::write(&subfolder, "keep")?;
    assert!(prepare(root.path(), RUN).is_err());
    assert_eq!(std::fs::read_to_string(&subfolder)?, "keep");
    std::fs::remove_file(&subfolder)?;
    let outside = tempfile::tempdir()?;
    symlink(outside.path(), &subfolder)?;
    assert!(prepare(root.path(), RUN).is_err());
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    Ok(())
}

#[test]
fn configured_storage_alias_resolves_to_the_same_canonical_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let storage = root.path().join("storage");
    std::fs::create_dir(&storage)?;
    let alias = root.path().join("alias");
    symlink(&storage, &alias)?;
    let canonical = prepare(&storage, RUN)?;
    assert_eq!(prepare(&alias, RUN)?, canonical);
    assert_eq!(canonical, evidence_folder(&storage, RUN));
    let new_path = prepare(&alias.join("new-briefs"), RUN)?;
    assert_eq!(new_path, evidence_folder(&storage.join("new-briefs"), RUN));
    Ok(())
}

#[test]
fn brief_reference_names_the_folder_the_run_and_the_note_tags_briefly() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let folder = prepare(root.path(), RUN)?;
    let brief = brief_reference(RUN, &folder);
    assert!(brief.contains(folder.to_string_lossy().as_ref()));
    assert!(brief.contains(EVIDENCE_ENV));
    assert!(brief.contains(RUN));
    for needle in [
        "session notes",
        "write_session_note",
        "list_session_notes",
        "acceptance-record",
        "acceptance-obligation",
        "comparisons/",
        "artifacts/",
        "never proof",
    ] {
        assert!(brief.contains(needle), "{needle}");
    }
    assert!(!brief.contains("manifest"));
    assert!(brief.is_ascii());
    let without_folder = brief.replace(folder.to_string_lossy().as_ref(), "");
    assert!(without_folder.len() < 1_000, "{}", without_folder.len());
    Ok(())
}
