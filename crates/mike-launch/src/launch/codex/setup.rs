use std::path::{Path, PathBuf};

const INSTALLER: &str = "scripts/codex-hooks/install";

pub(crate) fn installer() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok();
    installer_in(executable.as_deref(), Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn installer_in(executable: Option<&Path>, build_checkout: &Path) -> Option<PathBuf> {
    if let Some(executable) = executable {
        let canonical = executable.canonicalize().ok();
        for path in canonical.as_deref().into_iter().chain(Some(executable)) {
            if let Some(directory) = path.parent() {
                for prefix in [Some(directory), directory.parent()].into_iter().flatten() {
                    if let Some(installer) = complete_assets(&prefix.join("share/mike")) {
                        return Some(installer);
                    }
                }
            }
            if let Some(installer) = checkout_assets(path) {
                return Some(installer);
            }
        }
    }
    checkout_assets(build_checkout)
}

fn checkout_assets(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .filter(|root| root.join("crates/mike-codex/Cargo.toml").is_file())
        .find_map(complete_assets)
}

fn complete_assets(root: &Path) -> Option<PathBuf> {
    let installer = root.join(INSTALLER);
    (installer.is_file() && installer.with_file_name("mike-codex-hook").is_file())
        .then_some(installer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets(root: &Path) -> anyhow::Result<PathBuf> {
        let installer = root.join(INSTALLER);
        std::fs::create_dir_all(
            installer
                .parent()
                .ok_or_else(|| anyhow::anyhow!("missing parent"))?,
        )?;
        std::fs::write(&installer, "installer")?;
        std::fs::write(installer.with_file_name("mike-codex-hook"), "hook")?;
        Ok(installer)
    }

    #[test]
    fn packaged_assets_work_without_the_build_checkout() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let expected = assets(&directory.path().join("share/mike"))?;
        let executable = directory.path().join("bin/mike");
        assert_eq!(
            installer_in(Some(&executable), Path::new("/missing-build")),
            Some(expected)
        );
        Ok(())
    }

    #[test]
    fn checkout_fallback_requires_both_scripts_and_the_expected_workspace() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let expected = assets(directory.path())?;
        let build = directory.path().join("crates/mike-launch");
        assert_eq!(installer_in(None, &build), None);
        let marker = directory.path().join("crates/mike-codex/Cargo.toml");
        std::fs::create_dir_all(
            marker
                .parent()
                .ok_or_else(|| anyhow::anyhow!("missing parent"))?,
        )?;
        std::fs::write(marker, "[package]")?;
        assert_eq!(installer_in(None, &build), Some(expected.clone()));
        std::fs::remove_file(expected.with_file_name("mike-codex-hook"))?;
        assert_eq!(installer_in(None, &build), None);
        Ok(())
    }
}
