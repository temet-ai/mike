use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn refuse(_path: &Path) {}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn private(path: PathBuf) -> PathBuf {
    path
}

pub(crate) fn env(name: &str) -> Option<OsString> {
    std::env::var_os(name).map(|value| private(PathBuf::from(value)).into_os_string())
}

pub(crate) fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| private(dirs.home_dir().to_path_buf()))
}

#[cfg(test)]
pub(crate) fn private(path: PathBuf) -> PathBuf {
    if path.starts_with(std::env::temp_dir()) {
        return path;
    }
    match account_home().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(relative) => test_home().join(relative),
        None => path,
    }
}

#[cfg(test)]
fn test_home() -> PathBuf {
    std::env::temp_dir().join(format!("mike-test-home-{}", unsafe { libc::getuid() }))
}

#[cfg(test)]
pub(crate) fn refuse(path: &Path) {
    if path.starts_with(std::env::temp_dir()) {
        return;
    }
    if let Some(root) = live_roots().iter().find(|root| path.starts_with(root)) {
        panic!(
            "a test resolved {} inside the operator's live mike installation ({}); give the test a private HOME, XDG_STATE_HOME, XDG_CONFIG_HOME or config path",
            path.display(),
            root.display()
        );
    }
}

#[cfg(test)]
fn live_roots() -> &'static [std::path::PathBuf] {
    static ROOTS: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| {
        account_home()
            .map(|home| roots_in(&home))
            .unwrap_or_default()
    })
}

#[cfg(test)]
fn roots_in(home: &Path) -> Vec<std::path::PathBuf> {
    [
        ".local/state/mike",
        ".config/mike",
        ".local/share/mike",
        "Library/Application Support/mike",
    ]
    .iter()
    .map(|relative| home.join(relative))
    .collect()
}

#[cfg(test)]
fn account_home() -> Option<std::path::PathBuf> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;

    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buffer = vec![0 as libc::c_char; 16 * 1024];
    let mut found = std::ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() {
        return None;
    }
    let directory = unsafe { CStr::from_ptr(entry.assume_init().pw_dir) };
    Some(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
        directory.to_bytes(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_home_is_read_from_the_password_database_not_the_environment() {
        let home = account_home();
        assert!(home.as_deref().is_some_and(Path::is_absolute), "{home:?}");
        assert!(!live_roots().is_empty());
    }

    #[test]
    #[should_panic(expected = "inside the operator's live mike installation")]
    fn resolving_into_the_live_state_directory_panics_under_test() {
        let home = account_home().unwrap_or_default();
        refuse(&home.join(".local/state/mike/control.sock"));
    }

    #[test]
    #[should_panic(expected = "inside the operator's live mike installation")]
    fn the_default_installation_under_the_real_home_panics_under_test() {
        let home = account_home().unwrap_or_default();
        let _ =
            super::super::InstallationPaths::resolve(Some(&home.join(".config/mike/config.toml")));
    }

    #[test]
    fn the_default_installation_of_a_test_lands_in_a_private_home() -> anyhow::Result<()> {
        let paths = super::super::InstallationPaths::resolve(None)?;
        for dir in [&paths.config_dir, &paths.data_dir, &paths.state_dir] {
            assert!(dir.starts_with(test_home()), "{dir:?}");
        }
        assert!(home().is_some_and(|home| home.starts_with(test_home())));
        Ok(())
    }

    #[test]
    fn a_private_directory_passes() -> anyhow::Result<()> {
        let private = tempfile::tempdir()?;
        refuse(&private.path().join(".local/state/mike"));
        let selected = private.path().join("config.toml");
        let paths = super::super::InstallationPaths::resolve(Some(&selected))?;
        assert!(paths.state_dir.starts_with(private.path()));
        Ok(())
    }

    #[test]
    fn every_live_directory_kind_is_refused() {
        let roots = roots_in(Path::new("/home/someone"));
        for path in [
            "/home/someone/.local/state/mike/fleet",
            "/home/someone/.config/mike/config.toml",
            "/home/someone/.local/share/mike",
        ] {
            assert!(
                roots.iter().any(|root| Path::new(path).starts_with(root)),
                "{path}"
            );
        }
        assert!(
            !roots
                .iter()
                .any(|root| Path::new("/home/someone/.local/state/mike-audit").starts_with(root))
        );
    }
}
