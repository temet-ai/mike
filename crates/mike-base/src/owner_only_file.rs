#[cfg(unix)]
#[path = "owner_only_file/anchored.rs"]
pub mod anchored;

use std::path::{Path, PathBuf};

fn resolve_target(path: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(path) {
        return real;
    }
    match std::fs::read_link(path) {
        Ok(link) if link.is_absolute() => link,
        Ok(link) => path
            .parent()
            .map_or_else(|| link.clone(), |dir| dir.join(&link)),
        Err(_) => path.to_path_buf(),
    }
}

pub fn write(path: &Path, contents: &str) -> std::io::Result<()> {
    let target = resolve_target(path);
    let staged = staging_path_for(&target);
    let written = write_new(&staged, contents)
        .and_then(|()| std::fs::rename(&staged, &target))
        .and_then(|()| sync_parent(&target));
    if written.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    written
}

fn sync_parent(target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(target.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    Ok(())
}

fn staging_path_for(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".staged-{}", uuid::Uuid::new_v4()));
    target.with_file_name(name)
}

#[cfg(unix)]
fn write_new(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_new(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

#[cfg(all(test, unix))]
pub mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode_of(path: &Path) -> std::io::Result<u32> {
        Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
    }

    #[test]
    fn a_new_file_is_readable_by_its_owner_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("secret");
        write(&path, "value")?;
        assert_eq!(std::fs::read_to_string(&path)?, "value");
        assert_eq!(mode_of(&path)?, 0o600);
        Ok(())
    }

    #[test]
    fn a_world_readable_file_is_replaced_by_an_owner_only_one() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("secret");
        std::fs::write(&path, "a much longer stale value")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        write(&path, "new")?;
        assert_eq!(std::fs::read_to_string(&path)?, "new");
        assert_eq!(mode_of(&path)?, 0o600);
        Ok(())
    }

    #[test]
    fn writing_through_a_symlink_keeps_the_link_and_updates_its_target() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        std::fs::write(&real, "old")?;
        std::os::unix::fs::symlink(&real, &link)?;
        write(&link, "new")?;
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real)?, "new");
        assert_eq!(mode_of(&real)?, 0o600);
        Ok(())
    }

    #[test]
    fn writing_through_a_dangling_symlink_creates_its_target_and_keeps_the_link()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("real", &link)?;
        write(&link, "new")?;
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real)?, "new");
        Ok(())
    }

    #[test]
    fn nothing_is_left_behind_beside_the_file() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("secret");
        write(&path, "value")?;
        let names: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("secret")]);
        Ok(())
    }

    #[test]
    fn a_missing_directory_is_an_error() {
        let path = Path::new("/nonexistent-mike-test-dir/secret");
        assert!(write(path, "value").is_err());
    }
    #[test]
    fn an_interrupted_staging_write_leaves_the_previous_file_valid() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        write(&path, "persist_enabled = false\n")?;
        let staged = staging_path_for(&path);
        write_new(&staged, "persist_enabled = true\n")?;
        assert_eq!(std::fs::read_to_string(&path)?, "persist_enabled = false\n");
        assert_eq!(mode_of(&staged)?, 0o600);
        std::fs::remove_file(staged)?;
        Ok(())
    }
}
