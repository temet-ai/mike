use super::*;

#[test]
fn eligible_checkout_refuses_scratch_state_and_relative_paths() {
    let home = Path::new("/home/example");
    for refused in [
        "x",
        ".claude",
        "/home/example/.local/state/mike/seats/a/c4",
        "/srv/clones/tmp/c4-plain-s3",
        "/var/tmp/clone",
        "/nonexistent/repo",
    ] {
        assert!(!eligible_checkout(Path::new(refused), home), "{refused}");
    }
}

#[test]
fn eligible_checkout_accepts_a_real_checkout() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(checkout) = manifest
        .ancestors()
        .find(|candidate| candidate.join(".git").exists())
    else {
        return;
    };
    if format!("{}/", checkout.display()).contains("/tmp/") {
        return;
    }
    assert!(eligible_checkout(checkout, Path::new("/home/example")));
}

#[test]
fn local_checkout_prefers_the_recorded_path_then_this_machines_locations() {
    let here = |path: &Path| path.starts_with("/local");
    let roots = vec!["/elsewhere/proj".to_owned(), "/local/proj".to_owned()];
    assert_eq!(
        local_checkout("/local/recorded", &roots, &here),
        Some(PathBuf::from("/local/recorded"))
    );
    assert_eq!(
        local_checkout("/mnt/data/elsewhere/proj", &roots, &here),
        Some(PathBuf::from("/local/proj"))
    );
    assert_eq!(local_checkout("/mnt/data/elsewhere/proj", &[], &here), None);
}
