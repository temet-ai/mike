use std::path::{Path, PathBuf};

use crate::launch::LaunchError;

pub fn resolve(
    raw: &str,
    home: Option<&str>,
    cwd: Option<&Path>,
) -> Result<PathBuf, LaunchError> {
    if raw.is_empty() {
        return Err(LaunchError::Usage("the folder argument is empty".into()));
    }
    let expanded = expand_tilde(raw, home)?;
    let absolute = match (expanded.is_absolute(), cwd) {
        (true, _) => expanded,
        (false, Some(cwd)) => cwd.join(expanded),
        (false, None) => {
            return Err(LaunchError::Failed(format!(
                "cannot resolve the relative folder `{raw}`: the working directory is unavailable; pass an absolute path"
            )));
        }
    };
    let resolved = std::fs::canonicalize(&absolute)
        .map_err(|_| LaunchError::Failed(format!("no such directory: {raw}")))?;
    if !resolved.is_dir() {
        return Err(LaunchError::Failed(format!(
            "not a directory: {raw} (resolves to {})",
            resolved.display()
        )));
    }
    Ok(resolved)
}

pub fn resolve_from_env(raw: &str) -> Result<PathBuf, LaunchError> {
    let cwd = std::env::current_dir().ok();
    let home = std::env::var("HOME").ok();
    resolve(raw, home.as_deref(), cwd.as_deref())
}

fn expand_tilde(raw: &str, home: Option<&str>) -> Result<PathBuf, LaunchError> {
    let rest = match raw {
        "~" => Some(""),
        _ => raw.strip_prefix("~/"),
    };
    match rest {
        None => Ok(PathBuf::from(raw)),
        Some(rest) => {
            let home = home.filter(|h| !h.is_empty()).ok_or_else(|| {
                LaunchError::Failed(format!("cannot expand `{raw}`: HOME is not set"))
            })?;
            Ok(Path::new(home).join(rest))
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    #[test]
    fn resolve_tilde_relative_and_absolute_all_reach_the_same_directory() -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let project = home.path().join("proj");
        std::fs::create_dir(&project)?;
        let expected = std::fs::canonicalize(&project)?;
        let home_str = home.path().to_string_lossy().into_owned();
        assert_eq!(
            resolve("~/proj", Some(&home_str), Some(Path::new("/")))?,
            expected
        );
        assert_eq!(resolve("proj", None, Some(home.path()))?, expected);
        assert_eq!(
            resolve(&project.to_string_lossy(), None, Some(Path::new("/")))?,
            expected
        );
        assert_eq!(
            resolve("~", Some(&home_str), Some(Path::new("/")))?,
            std::fs::canonicalize(home.path())?
        );
        Ok(())
    }

    #[test]
    fn resolve_missing_directory_is_no_such_directory() {
        let err = resolve("/nonexistent/x/y", None, Some(Path::new("/"))).unwrap_err();
        assert!(err.to_string().contains("no such directory"), "{err}");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn resolve_file_is_not_a_directory() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("f");
        std::fs::write(&file, "x")?;
        let err = resolve(&file.to_string_lossy(), None, Some(Path::new("/"))).unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
        Ok(())
    }

    #[test]
    fn resolve_tilde_without_home_and_empty_are_errors() {
        assert!(resolve("~/x", None, Some(Path::new("/"))).is_err());
        assert!(resolve("", None, Some(Path::new("/"))).is_err());
    }

    #[test]
    fn resolve_follows_symlinks_to_the_real_path() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("real");
        std::fs::create_dir(&real)?;
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link)?;
        assert_eq!(
            resolve(&link.to_string_lossy(), None, Some(Path::new("/")))?,
            std::fs::canonicalize(&real)?
        );
        Ok(())
    }

    #[test]
    fn resolve_absolute_folder_ignores_a_deleted_working_directory() -> anyhow::Result<()> {
        let project = tempfile::tempdir()?;
        let gone = tempfile::tempdir()?;
        let gone_path = gone.path().to_path_buf();
        drop(gone);
        let expected = std::fs::canonicalize(project.path())?;
        let raw = project.path().to_string_lossy();
        assert_eq!(resolve(&raw, None, Some(&gone_path))?, expected);
        assert_eq!(resolve(&raw, None, None)?, expected);
        Ok(())
    }

    #[test]
    fn resolve_relative_folder_without_a_working_directory_asks_for_an_absolute_path() {
        let err = resolve("proj", None, None).unwrap_err();
        assert!(err.to_string().contains("pass an absolute path"), "{err}");
        assert_eq!(err.exit_code(), 1);
    }
}
