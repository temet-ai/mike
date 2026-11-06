use std::path::Path;

use anyhow::{Context, Result};

use super::super::{absolute, companion, inspect_url};

#[test]
fn selected_inspect_address_is_used_including_ipv6_and_wildcards() -> Result<()> {
    for (address, expected) in [
        ("localhost:14589", "http://127.0.0.1:14589"),
        ("LOCALHOST:14589", "http://127.0.0.1:14589"),
        ("127.0.0.1:14589", "http://127.0.0.1:14589"),
        ("0.0.0.0:14589", "http://127.0.0.1:14589"),
        ("[::]:14589", "http://[::1]:14589"),
        ("[::1]:14589", "http://[::1]:14589"),
    ] {
        let mut config = crate::config::ProxyConfig::default();
        config.inspect_listen_addr = address.into();
        config.unsupported_allow_remote_listeners =
            address.starts_with("0.0.0.0:") || address.starts_with("[::]:");
        assert_eq!(inspect_url(&config)?, expected);
    }
    let mut config = crate::config::ProxyConfig::default();
    config.inspect_listen_addr = "broken".into();
    assert!(inspect_url(&config).is_err());
    Ok(())
}

#[test]
fn absolute_paths_normalize_parent_components_and_reject_empty() -> Result<()> {
    assert_eq!(absolute(Path::new("/tmp/a/../b"))?, Path::new("/tmp/b"));
    assert!(absolute(Path::new("")).is_err());
    assert!(absolute(Path::new("relative path"))?.is_absolute());
    Ok(())
}

#[test]
fn companion_missing_or_immutable_version_path_is_rejected() -> Result<()> {
    let dir = tempfile::tempdir()?;
    assert!(companion::validate(&dir.path().join("missing")).is_err());
    assert!(companion::validate(dir.path()).is_err());
    for path in [
        "/prefix/Cellar/mike-mcp/1.0/bin/mike-mcp",
        "/prefix/lib/mike-archive/generations/abc/mike-mcp",
    ] {
        assert!(companion::validate(Path::new(path)).is_err());
    }
    Ok(())
}

#[test]
fn archive_and_split_homebrew_layout_prefer_stable_prefix() {
    let archive = companion::candidates(
        Path::new("/prefix with spaces/lib/mike-archive/generations/abc/mike"),
        None,
        None,
    );
    assert_eq!(archive[0], Path::new("/prefix with spaces/bin/mike-mcp"));
    let brew = companion::candidates(
        Path::new("/opt/homebrew/Cellar/mike/1.0/bin/mike"),
        None,
        None,
    );
    assert_eq!(brew[0], Path::new("/opt/homebrew/bin/mike-mcp"));
    assert_eq!(
        brew[1],
        Path::new("/opt/homebrew/opt/mike-mcp/bin/mike-mcp")
    );
}

#[cfg(unix)]
#[test]
fn companion_keeps_generation_and_homebrew_symlinks_stable() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir()?;
    let stable = dir.path().join("bin with spaces/mike-mcp");
    let generation = dir.path().join("lib/mike-archive/generations/abc/mike-mcp");
    std::fs::create_dir_all(generation.parent().context("generation parent")?)?;
    std::fs::create_dir_all(stable.parent().context("stable parent")?)?;
    std::fs::write(&generation, "binary")?;
    std::fs::set_permissions(&generation, std::fs::Permissions::from_mode(0o700))?;
    symlink(&generation, &stable)?;
    assert_eq!(companion::validate(&stable)?, stable);
    assert_ne!(
        companion::validate(&stable)?,
        std::fs::canonicalize(&stable)?
    );
    std::fs::set_permissions(generation, std::fs::Permissions::from_mode(0o600))?;
    assert!(companion::validate(&stable).is_err());
    Ok(())
}
