use super::*;
use crate::launch::backend::fake::FakeBackend;

#[cfg_attr(test, test)]
pub(crate) fn foreign_origin_and_witness_loss_never_prove_exit() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let mut surface = FakeBackend::surface(Some(session));
    let key = RootKey::from_surface(1, "fleet-origin", session, &surface)
        .ok_or_else(|| anyhow::anyhow!("fixture key"))?;
    assert_eq!(witnessed_exit(false, Some(false)), None);
    assert_eq!(witnessed_exit(false, Some(true)), None);
    assert!(matching_surface(&key, &surface));
    surface.backend = BackendId::Tmux;
    assert!(!matching_surface(&key, &surface));
    surface.backend = BackendId::Herdr;
    surface.session_uuid = Some(Uuid::new_v4());
    assert!(!matching_surface(&key, &surface));
    surface.session_uuid = Some(session);
    surface.pid = None;
    assert!(!matching_surface(&key, &surface));
    surface.pid = Some(4243);
    assert!(!matching_surface(&key, &surface));
    surface.pid = Some(4242);
    surface.surface_ref = "foreign-pane".into();
    assert!(!matching_surface(&key, &surface));
    assert_witness_observations(&key);
    Ok(())
}

fn assert_witness_observations(key: &RootKey) {
    let witness = HashSet::from([key.clone()]);
    assert_eq!(
        witnessed_exit(witness.contains(key), Some(true)),
        Some(false)
    );
    assert_eq!(
        witnessed_exit(witness.contains(key), Some(false)),
        Some(true)
    );
    assert_eq!(witnessed_exit(witness.contains(key), None), None);
    assert_eq!(presence_from_probe(-1, Some(libc::EPERM)), None);
    assert_eq!(presence_from_probe(-1, Some(libc::ESRCH)), Some(false));
    assert_eq!(presence_from_probe(-1, Some(libc::EINVAL)), None);
    let restarted = HashSet::<RootKey>::new();
    assert_eq!(witnessed_exit(restarted.contains(key), Some(false)), None);
}

#[cfg_attr(test, test)]
pub(crate) fn linux_and_macos_marker_parsers_require_exact_identity() {
    let session = Uuid::new_v4();
    let run = "fleet-origin";
    let env = format!("MIKE_SESSION_ID={session}\0MIKE_RUN_ID={run}\0");
    assert!(environment_matches(env.as_bytes(), session, run));
    assert!(!environment_matches(env.as_bytes(), Uuid::new_v4(), run));
    assert!(!environment_matches(env.as_bytes(), session, "foreign-run"));
    let duplicate = format!("{env}MIKE_RUN_ID={run}\0");
    assert!(!environment_matches(duplicate.as_bytes(), session, run));
    let mut args = 2_i32.to_ne_bytes().to_vec();
    args.extend_from_slice(b"/bin/sh\0\0sh\0-c\0");
    args.extend_from_slice(env.as_bytes());
    args.push(0);
    assert!(macos::markers_match(&args, session, run));
    assert!(!macos::markers_match(&args, session, "foreign-run"));
    assert!(!macos::markers_match(&[], session, run));
    let mut redacted = 1_i32.to_ne_bytes().to_vec();
    redacted.extend_from_slice(b"/bin/sh\0sh\0");
    assert!(!macos::markers_match(&redacted, session, run));
    let mut spoofed_argv = 2_i32.to_ne_bytes().to_vec();
    spoofed_argv.extend_from_slice(b"/bin/sh\0");
    spoofed_argv.extend_from_slice(env.as_bytes());
    assert!(!macos::markers_match(&spoofed_argv, session, run));
    assert_eq!(origin_policy("linux"), "local-environment/session-run");
    assert_eq!(
        origin_policy("macos"),
        "local-sysctl/session-run/macos-runtime-unvalidated"
    );
    assert_eq!(
        origin_policy("windows"),
        "unknown/platform-local-origin-unimplemented"
    );
}

#[cfg(feature = "test-seams")]
pub(crate) fn remember_owned_fixture(launch: i64, run: &str, session: Uuid, pid: u32) -> bool {
    let mut surface = FakeBackend::surface(Some(session));
    surface.pid = Some(pid);
    remember_opened(Some(launch), Some(run), Some(session), &surface);
    RootKey::from_surface(launch, run, session, &surface).and_then(|key| observed_exit(&key))
        == Some(false)
}
