mod apply_tests;
mod atomic_tests;
mod mcp_tests;
mod mods_tests;
mod probe_tests;
mod shell_tests;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::locations::Locations;

pub(super) struct Machine {
    pub(super) dir: tempfile::TempDir,
    pub(super) locations: Locations,
    pub(super) bin: PathBuf,
}

impl Machine {
    pub(super) fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let home = std::fs::canonicalize(dir.path())?.join("home");
        let bin = std::fs::canonicalize(dir.path())?.join("bin");
        std::fs::create_dir_all(&home)?;
        std::fs::create_dir_all(&bin)?;
        let path_var: OsString = bin.clone().into_os_string();
        let locations = Locations::for_home(&home, Some(path_var));
        Ok(Self {
            dir,
            locations,
            bin,
        })
    }

    pub(super) fn home(&self) -> &Path {
        &self.locations.home
    }

    pub(super) fn add_executable(&self, name: &str, body: &str) -> anyhow::Result<PathBuf> {
        let path = self.bin.join(name);
        std::fs::write(&path, body)?;
        make_executable(&path)?;
        Ok(path)
    }
}

#[cfg(unix)]
pub(super) fn make_executable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn make_executable(_: &Path) -> anyhow::Result<()> {
    Ok(())
}
