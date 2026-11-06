use std::os::unix::fs::symlink;

use super::*;

const RUN: &str = "fleet-project-a1234567";

fn manifest_path(root: &Path, run: &str) -> PathBuf {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .join("fleet-runs")
        .join(run)
        .join("acceptance/manifest.json")
}

#[test]
fn run_scratch_starts_without_claiming_observations_or_artifact_proof() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let session = Uuid::from_u128(1);
    let path = prepare(root.path(), RUN, Some(session))?;
    assert_eq!(path, manifest_path(root.path(), RUN));
    let value: Value = serde_json::from_reader(File::open(&path)?)?;
    assert_eq!(value["version"], VERSION);
    assert_eq!(value["run_id"], RUN);
    assert_eq!(value["created_by_session"], session.to_string());
    assert_eq!(value["comparisons"], json!([]));
    assert_eq!(value["artifacts"], json!([]));
    assert!(value["close_handoff"].is_null());
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("no parent"))?;
    assert!(parent.join("comparisons").is_dir());
    assert!(parent.join("artifacts").is_dir());
    Ok(())
}

#[test]
fn successor_reuses_full_manifest_scratch_and_handoff_without_rewriting() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = prepare(root.path(), RUN, Some(Uuid::from_u128(1)))?;
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("no parent"))?;
    let scratch = parent.join("comparisons/live.json");
    let payload = "live observation\n".repeat(10_000);
    std::fs::write(&scratch, &payload)?;
    let mut value: Value = serde_json::from_reader(File::open(&path)?)?;
    value["comparisons"] = json!([{"path": scratch, "outcome": "not_exercised"}]);
    value["artifacts"] = json!([{"sha": null, "features": null, "environment": null}]);
    value["close_handoff"] = json!({"note": Uuid::from_u128(9)});
    let saved = serde_json::to_vec_pretty(&value)?;
    std::fs::write(&path, &saved)?;
    let successor = prepare(root.path(), RUN, Some(Uuid::from_u128(2)))?;
    assert_eq!(successor, path);
    assert_eq!(std::fs::read(&path)?, saved);
    assert_eq!(std::fs::read_to_string(&scratch)?, payload);
    let brief = brief_reference(&successor);
    assert!(brief.contains(path.to_string_lossy().as_ref()));
    assert!(!brief.contains("live observation"));
    assert!(!brief.contains(&Uuid::from_u128(9).to_string()));
    assert!(brief.len() < 2_000);
    Ok(())
}

#[test]
fn different_runs_never_share_acceptance_artifacts() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let first = prepare(root.path(), RUN, None)?;
    let second = prepare(root.path(), "fleet-project-b1234567", None)?;
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
    Ok(())
}

#[test]
fn concurrent_launches_publish_one_complete_manifest() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let result = std::thread::scope(|scope| {
        let mut threads = Vec::new();
        for id in 1..9 {
            let path = root.path();
            threads.push(scope.spawn(move || prepare(path, RUN, Some(Uuid::from_u128(id)))));
        }
        for thread in threads {
            let path = thread
                .join()
                .map_err(|_| anyhow::anyhow!("launch panicked"))??;
            assert_eq!(path, manifest_path(root.path(), RUN));
        }
        Ok::<_, anyhow::Error>(())
    });
    result?;
    let value: Value = serde_json::from_reader(File::open(manifest_path(root.path(), RUN))?)?;
    assert_eq!(value["run_id"], RUN);
    let creator: Uuid = serde_json::from_value(value["created_by_session"].clone())?;
    assert!((1..9).contains(&creator.as_u128()));
    Ok(())
}

#[test]
fn unsafe_run_and_relative_storage_are_refused_before_creating_files() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for run in [
        "fleet-../escape",
        "fleet-x/y",
        "fleet-x\\y",
        "fleet-",
        "../fleet-x",
        "fleet-x\n",
    ] {
        assert!(prepare(root.path(), run, None).is_err(), "{run}");
    }
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    assert!(prepare(Path::new("relative"), RUN, None).is_err());
    assert!(prepare(&root.path().join("../escape"), RUN, None).is_err());
    Ok(())
}

#[test]
fn symlinked_storage_components_and_file_collisions_fail_safely() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let path = root.path().join("fleet-runs");
    symlink(outside.path(), &path)?;
    assert!(prepare(root.path(), RUN, None).is_err());
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    std::fs::remove_file(&path)?;
    std::fs::write(&path, "keep")?;
    assert!(prepare(root.path(), RUN, None).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "keep");
    Ok(())
}

#[test]
fn symlinked_manifest_never_reads_or_replaces_its_target() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = manifest_path(root.path(), RUN);
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("no parent"))?;
    directory(parent)?;
    let target = root.path().join("other.json");
    let original = serde_json::to_vec(&initial(RUN, None))?;
    std::fs::write(&target, &original)?;
    symlink(&target, &path)?;
    assert!(prepare(root.path(), RUN, None).is_err());
    assert!(std::fs::symlink_metadata(&path)?.file_type().is_symlink());
    assert_eq!(std::fs::read(&target)?, original);
    Ok(())
}

#[test]
fn invalid_existing_manifest_is_preserved_and_refuses_launch() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = prepare(root.path(), RUN, None)?;
    for content in [
        "not json".to_string(),
        initial("fleet-other", None).to_string(),
        json!({"version": 2, "run_id": RUN, "comparisons": [], "artifacts": []}).to_string(),
        json!({"version": 1, "run_id": RUN}).to_string(),
    ] {
        std::fs::write(&path, &content)?;
        assert!(prepare(root.path(), RUN, None).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, content);
    }
    std::fs::remove_file(&path)?;
    std::fs::create_dir(&path)?;
    assert!(prepare(root.path(), RUN, None).is_err());
    assert!(path.is_dir());
    Ok(())
}

#[test]
fn missing_storage_and_non_regular_manifests_report_io_errors() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let missing = root.path().join("missing/manifest.json");
    assert!(publish(&missing, RUN, None).is_err());
    assert!(read_manifest(&missing).is_err());
    assert!(verify(&missing, RUN).is_err());
    assert!(read_manifest(root.path()).is_err());
    Ok(())
}

#[test]
fn configured_storage_alias_resolves_to_the_same_absolute_manifest() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let storage = root.path().join("storage");
    std::fs::create_dir(&storage)?;
    let alias = root.path().join("alias");
    symlink(&storage, &alias)?;
    let canonical = prepare(&storage, RUN, None)?;
    assert_eq!(prepare(&alias, RUN, None)?, canonical);
    assert_eq!(canonical, manifest_path(&storage, RUN));
    let new_base = alias.join("new-briefs");
    let new_path = prepare(&new_base, RUN, None)?;
    assert_eq!(new_path, manifest_path(&storage.join("new-briefs"), RUN));
    Ok(())
}

#[test]
fn oversized_manifest_fails_explicitly_without_rewriting_or_dropping_data() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = prepare(root.path(), RUN, None)?;
    let mut value = initial(RUN, None);
    value["payload"] = json!("x".repeat(MAX_MANIFEST_BYTES as usize));
    let saved = serde_json::to_vec(&value)?;
    std::fs::write(&path, &saved)?;
    let error = prepare(root.path(), RUN, None)
        .err()
        .ok_or_else(|| anyhow::anyhow!("accepted huge manifest"))?;
    assert!(error.to_string().contains("exceeds"));
    assert_eq!(std::fs::read(&path)?, saved);
    Ok(())
}
