use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn fixture() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "persist_enabled=false\n")?;
    Ok((dir, path))
}

#[test]
fn concurrent_disjoint_updates_both_survive() -> Result<()> {
    let (_dir, path) = fixture()?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let updates = [
        "proxy_listen_addr='127.0.0.1:7001'",
        "inspect_listen_addr='127.0.0.1:7002'",
    ];
    let handles: Vec<_> = updates
        .into_iter()
        .map(|update| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || -> Result<()> {
                barrier.wait();
                super::super::write::update_selected(&path, update.parse()?)?;
                Ok(())
            })
        })
        .collect();
    barrier.wait();
    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("configuration writer thread panicked"))??;
    }
    let saved: toml::Table = std::fs::read_to_string(&path)?.parse()?;
    assert_eq!(saved["proxy_listen_addr"].as_str(), Some("127.0.0.1:7001"));
    assert_eq!(
        saved["inspect_listen_addr"].as_str(),
        Some("127.0.0.1:7002")
    );
    assert_eq!(saved["persist_enabled"].as_bool(), Some(false));
    Ok(())
}

#[test]
fn changed_source_is_refused_without_replacing_noncooperating_edit() -> Result<()> {
    let (_dir, path) = fixture()?;
    let transaction = Transaction::begin(&path)?;
    assert_eq!(
        transaction.table()?["persist_enabled"].as_bool(),
        Some(false)
    );
    let other = "persist_enabled=true\nproxy_listen_addr='127.0.0.1:7009'\n";
    std::fs::write(&path, other)?;
    assert!(transaction.commit("persist_enabled=false\n").is_err());
    assert_eq!(std::fs::read_to_string(&path)?, other);
    Ok(())
}

#[test]
fn replaced_source_with_identical_bytes_is_refused() -> Result<()> {
    let (dir, path) = fixture()?;
    let transaction = Transaction::begin(&path)?;
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, "persist_enabled=false\n")?;
    std::fs::rename(&replacement, &path)?;
    assert!(transaction.commit("persist_enabled=true\n").is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "persist_enabled=false\n");
    Ok(())
}

#[test]
fn changed_source_symlink_never_redirects_the_private_write() -> Result<()> {
    let (dir, path) = fixture()?;
    let outside = dir.path().join("unrelated");
    std::fs::write(&outside, "unrelated-data")?;
    let transaction = Transaction::begin(&path)?;
    std::fs::remove_file(&path)?;
    symlink(&outside, &path)?;
    assert!(transaction.commit("persist_enabled=true\n").is_err());
    assert_eq!(std::fs::read_to_string(&outside)?, "unrelated-data");
    assert!(std::fs::symlink_metadata(&path)?.file_type().is_symlink());
    Ok(())
}

#[test]
fn renamed_parent_is_refused_without_writing_into_the_replacement_directory() -> Result<()> {
    let (dir, _) = fixture()?;
    let parent = dir.path().join("selected");
    let path = parent.join("config.toml");
    std::fs::create_dir(&parent)?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
    std::fs::write(&path, "persist_enabled=false\n")?;
    let transaction = Transaction::begin(&path)?;
    let moved = dir.path().join("moved");
    std::fs::rename(&parent, &moved)?;
    std::fs::create_dir(&parent)?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
    std::fs::write(&path, "unrelated-data")?;
    assert!(transaction.commit("persist_enabled=true\n").is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "unrelated-data");
    assert_eq!(
        std::fs::read_to_string(moved.join("config.toml"))?,
        "persist_enabled=false\n"
    );
    Ok(())
}

#[test]
fn absent_source_created_outside_lock_is_not_overwritten() -> Result<()> {
    let (_dir, path) = fixture()?;
    std::fs::remove_file(&path)?;
    let transaction = Transaction::begin(&path)?;
    std::fs::write(&path, "persist_enabled=true\n")?;
    assert!(transaction.commit("persist_enabled=false\n").is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "persist_enabled=true\n");
    Ok(())
}
