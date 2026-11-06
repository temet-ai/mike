use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use uuid::Uuid;

use super::{BackendId, Surface};

#[cfg(any(target_os = "macos", test, feature = "test-seams"))]
#[path = "local_root_macos.rs"]
mod macos;

static WITNESSES: LazyLock<Mutex<HashSet<RootKey>>> = LazyLock::new(Mutex::default);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RootKey {
    launch: i64,
    run: String,
    backend: BackendId,
    session: Uuid,
    surface: String,
    pid: i32,
}

impl RootKey {
    pub(crate) fn from_record(
        launch: i64,
        run: &str,
        backend: &str,
        session: Option<Uuid>,
        surface: Option<&str>,
        pid: Option<i32>,
    ) -> Option<Self> {
        let key = Self {
            launch,
            run: run.into(),
            backend: backend.parse().ok()?,
            session: session?,
            surface: surface?.into(),
            pid: pid?,
        };
        (launch > 0
            && key.pid > 0
            && matches!(
                key.backend,
                BackendId::Herdr | BackendId::Tmux | BackendId::Terminal
            ))
        .then_some(key)
    }

    fn from_surface(launch: i64, run: &str, session: Uuid, surface: &Surface) -> Option<Self> {
        if surface.session_uuid != Some(session) {
            return None;
        }
        Self::from_record(
            launch,
            run,
            surface.backend.as_str(),
            Some(session),
            Some(&surface.surface_ref),
            surface.pid.and_then(|pid| i32::try_from(pid).ok()),
        )
    }
}

pub(crate) fn remember_opened(
    launch: Option<i64>,
    run: Option<&str>,
    session: Option<Uuid>,
    surface: &Surface,
) {
    let Some(key) = launch
        .zip(run)
        .zip(session)
        .and_then(|((id, run), session)| RootKey::from_surface(id, run, session, surface))
    else {
        return;
    };
    bind_current(&key, surface);
}

pub(crate) fn bind_current(key: &RootKey, surface: &Surface) {
    let matching = matching_surface(key, surface);
    if matching && attested(key) && process_present(key.pid) == Some(true) {
        WITNESSES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key.clone());
    }
}

fn matching_surface(key: &RootKey, surface: &Surface) -> bool {
    RootKey::from_surface(key.launch, &key.run, key.session, surface)
        .is_some_and(|current| current == *key)
}

pub(crate) fn observed_exit(key: &RootKey) -> Option<bool> {
    let witnessed = WITNESSES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(key);
    if !witnessed {
        return None;
    }
    witnessed_exit(true, process_present(key.pid))
}

fn witnessed_exit(witnessed: bool, present: Option<bool>) -> Option<bool> {
    if witnessed {
        present.map(|value| !value)
    } else {
        None
    }
}

pub(crate) fn forget(key: &RootKey) {
    WITNESSES
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(key);
}

fn origin_policy(platform: &str) -> &'static str {
    match platform {
        "linux" => "local-environment/session-run",
        "macos" => "local-sysctl/session-run/macos-runtime-unvalidated",
        _ => "unknown/platform-local-origin-unimplemented",
    }
}

pub(crate) fn capability() -> &'static str {
    origin_policy(std::env::consts::OS)
}

#[cfg(target_os = "linux")]
fn attested(key: &RootKey) -> bool {
    let Ok(mut bytes) = std::fs::read(format!("/proc/{}/environ", key.pid)) else {
        return false;
    };
    let matched = environment_matches(&bytes, key.session, &key.run);
    bytes.fill(0);
    matched
}

#[cfg(target_os = "macos")]
fn attested(key: &RootKey) -> bool {
    macos::attested(key.pid, key.session, &key.run)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn attested(_key: &RootKey) -> bool {
    false
}

#[cfg(any(target_os = "linux", target_os = "macos", test, feature = "test-seams"))]
fn environment_matches(bytes: &[u8], session: Uuid, run: &str) -> bool {
    if !bytes.ends_with(&[0]) {
        return false;
    }
    unique_environment_value(bytes, b"MIKE_SESSION_ID=") == Some(session.to_string().as_bytes())
        && unique_environment_value(bytes, b"MIKE_RUN_ID=") == Some(run.as_bytes())
}

#[cfg(any(target_os = "linux", target_os = "macos", test, feature = "test-seams"))]
fn unique_environment_value<'a>(bytes: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut matches = bytes
        .split(|byte| *byte == 0)
        .filter_map(|value| value.strip_prefix(name));
    let value = matches.next()?;
    matches.next().is_none().then_some(value)
}

#[cfg(unix)]
pub(crate) fn process_present(pid: i32) -> Option<bool> {
    if pid <= 0 {
        return None;
    }
    let result = unsafe { libc::kill(pid, 0) };
    presence_from_probe(result, std::io::Error::last_os_error().raw_os_error())
}

#[cfg(unix)]
fn presence_from_probe(result: i32, error: Option<i32>) -> Option<bool> {
    match (result, error) {
        (0, _) => Some(true),
        (_, Some(libc::ESRCH)) => Some(false),
        _ => None,
    }
}

#[cfg(not(unix))]
pub(crate) fn process_present(_pid: i32) -> Option<bool> {
    None
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "local_root_tests.rs"]
pub(crate) mod tests;
