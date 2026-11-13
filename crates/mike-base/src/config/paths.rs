use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn process_lookup(name: &str) -> Option<String> {
    live_install_guard::env(name).map(|value| value.to_string_lossy().into_owned())
}

#[path = "live_install_guard.rs"]
pub mod live_install_guard;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationPaths {
    pub config_file: PathBuf,
    pub installation_id: String,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl InstallationPaths {
    pub fn resolve(selected: Option<&Path>) -> Result<Self> {
        Self::resolve_in(selected, &process_lookup)
    }

    pub fn resolve_in(selected: Option<&Path>, environment: Lookup<'_>) -> Result<Self> {
        let paths = Self::resolve_unguarded(selected, environment)?;
        for dir in [&paths.config_dir, &paths.data_dir, &paths.state_dir] {
            live_install_guard::refuse(dir);
        }
        Ok(paths)
    }

    fn resolve_unguarded(selected: Option<&Path>, environment: Lookup<'_>) -> Result<Self> {
        if let Some(path) = selected {
            let config_file = absolute(path)?;
            return Self::selected(config_file, Self::resolve_unguarded(None, environment).ok());
        }
        let dirs = directories::ProjectDirs::from("", "", "mike")
            .context("cannot resolve user directories; supply --config with an absolute path")?;
        let home = environment_value(environment, "HOME").map(PathBuf::from);
        let user_root = |variable: &str, fallback: &str| -> Result<PathBuf> {
            environment_value(environment, variable)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| home.as_ref().map(|path| path.join(fallback)))
                .map(|path| path.join("mike"))
                .context("cannot resolve user directories; supply --config with an absolute path")
        };
        let config_dir = match absolute_env(environment, CONFIG_HOME_ENV) {
            Some(root) => root.join("mike"),
            None => user_root("XDG_CONFIG_HOME", ".config")?,
        };
        let state_dir = user_root("XDG_STATE_HOME", ".local/state")?;
        Ok(Self::platform_default(
            config_dir,
            live_install_guard::private(dirs.data_dir().to_path_buf()),
            state_dir,
        ))
    }

    fn platform_default(config_dir: PathBuf, data_dir: PathBuf, state_dir: PathBuf) -> Self {
        let config_file = config_dir.join("config.toml");
        Self {
            installation_id: identity(&config_file),
            config_file,
            config_dir,
            data_dir,
            log_dir: state_dir.join("logs"),
            state_dir,
        }
    }

    fn selected(config_file: PathBuf, default: Option<Self>) -> Result<Self> {
        if let Some(default) = default
            && config_file == default.config_file
        {
            return Ok(default);
        }
        let config_dir = config_file
            .parent()
            .context("config needs a parent directory")?
            .to_path_buf();
        let installation_id = identity(&config_file);
        let root = config_dir.join(format!(".mike-{installation_id}"));
        Ok(Self {
            installation_id,
            data_dir: root.join("data"),
            state_dir: root.join("state"),
            log_dir: root.join("logs"),
            config_file,
            config_dir,
        })
    }
}

pub const CONFIG_HOME_ENV: &str = "MIKE_CONFIG_HOME";

pub fn environment_value(environment: Lookup<'_>, name: &str) -> Option<std::ffi::OsString> {
    environment(name)
        .map(|value| live_install_guard::private(PathBuf::from(value)).into_os_string())
}

fn absolute_env(environment: Lookup<'_>, name: &str) -> Option<PathBuf> {
    environment_value(environment, name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    anyhow::ensure!(
        !path.as_os_str().is_empty(),
        "config path must not be empty"
    );
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("cannot resolve relative --config path")?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn identity(path: &Path) -> String {
    use sha2::Digest;
    hex::encode(&sha2::Sha256::digest(path.as_os_str().as_encoded_bytes())[..16])
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn explicit_installation_paths_follow_the_selected_file() -> Result<()> {
        let paths = InstallationPaths::resolve(Some(Path::new("/tmp/mike space/config.toml")))?;
        let root = Path::new("/tmp/mike space").join(format!(".mike-{}", paths.installation_id));
        assert_eq!(paths.data_dir, root.join("data"));
        assert_eq!(paths.state_dir, root.join("state"));
        assert_eq!(paths.log_dir, root.join("logs"));
        Ok(())
    }

    #[test]
    fn empty_selection_is_rejected() {
        assert!(InstallationPaths::resolve(Some(Path::new(""))).is_err());
    }
    #[test]
    fn two_selected_files_in_one_directory_have_distinct_private_state() -> Result<()> {
        let a = InstallationPaths::resolve(Some(Path::new("/tmp/mike/a.toml")))?;
        let b = InstallationPaths::resolve(Some(Path::new("/tmp/mike/b.toml")))?;
        assert_ne!(a.installation_id, b.installation_id);
        assert_ne!(a.state_dir, b.state_dir);
        assert_ne!(a.data_dir, b.data_dir);
        assert_ne!(a.log_dir, b.log_dir);
        let normalized =
            InstallationPaths::resolve(Some(Path::new("/tmp/mike/elsewhere/../a.toml")))?;
        assert_eq!(normalized, a);
        Ok(())
    }

    #[test]
    fn shared_platform_roots_are_preserved_even_when_selected_explicitly() -> Result<()> {
        let root = PathBuf::from("/Users/fixture/Library/Application Support/mike");
        let default =
            InstallationPaths::platform_default(root.clone(), root.clone(), root.join("state"));
        assert_eq!(default.config_file, root.join("config.toml"));
        assert_eq!(default.data_dir, root);
        assert_eq!(default.state_dir, default.data_dir.join("state"));
        let selected =
            InstallationPaths::selected(default.config_file.clone(), Some(default.clone()))?;
        assert_eq!(selected, default);
        Ok(())
    }

    #[test]
    fn distinct_configuration_has_nonoverlapping_maintenance_roots() -> Result<()> {
        let root = PathBuf::from("/Users/fixture/Library/Application Support/mike");
        let default =
            InstallationPaths::platform_default(root.clone(), root.clone(), root.join("state"));
        let selected = InstallationPaths::selected(
            root.join("archive-install/config.toml"),
            Some(default.clone()),
        )?;
        assert_ne!(selected.installation_id, default.installation_id);
        let roots = [
            &selected.config_file,
            &selected.data_dir,
            &selected.state_dir,
        ];
        for (index, path) in roots.iter().enumerate() {
            for other in roots.iter().skip(index + 1) {
                assert!(!path.starts_with(other) && !other.starts_with(path));
            }
        }
        assert_eq!(default.data_dir, root);
        Ok(())
    }

    #[test]
    fn selected_layout_is_read_only_and_stable_across_first_save_and_runtime_state() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = std::fs::canonicalize(temp.path())?;
        let file = root.join("new installation/config.toml");
        let before = InstallationPaths::resolve(Some(&file))?;
        assert!(!before.config_dir.exists());
        std::fs::create_dir_all(&before.data_dir)?;
        std::fs::create_dir_all(&before.state_dir)?;
        std::fs::write(&file, "persist_enabled=false\n")?;
        let sentinel = before.data_dir.join("retained-body");
        std::fs::write(&sentinel, b"synthetic retained bytes")?;
        assert_eq!(InstallationPaths::resolve(Some(&file))?, before);
        assert_eq!(std::fs::read(&sentinel)?, b"synthetic retained bytes");
        assert_eq!(std::fs::read_to_string(&file)?, "persist_enabled=false\n");
        Ok(())
    }

    #[test]
    fn distinct_linux_platform_defaults_remain_unchanged() {
        let paths = InstallationPaths::platform_default(
            PathBuf::from("/fixture/config/mike"),
            PathBuf::from("/fixture/data/mike"),
            PathBuf::from("/fixture/state/mike"),
        );
        assert_eq!(
            paths.config_file,
            Path::new("/fixture/config/mike/config.toml")
        );
        assert_eq!(paths.data_dir, Path::new("/fixture/data/mike"));
        assert_eq!(paths.state_dir, Path::new("/fixture/state/mike"));
        assert_eq!(paths.log_dir, Path::new("/fixture/state/mike/logs"));
    }

    #[test]
    fn selected_root_without_a_parent_is_refused() {
        assert!(InstallationPaths::selected(PathBuf::from("/"), None).is_err());
    }
}
