use super::*;

#[test]
fn config_file_follows_xdg_config_home_when_it_is_set() -> anyhow::Result<()> {
    assert_eq!(
        config_file_from(Some("/srv/xdg"), Some("/home/ada"))?,
        PathBuf::from("/srv/xdg/mike/config.toml")
    );
    Ok(())
}

#[test]
fn config_file_is_under_dot_config_on_linux_without_xdg() -> anyhow::Result<()> {
    assert_eq!(
        config_file_from(None, Some("/home/ada"))?,
        PathBuf::from("/home/ada/.config/mike/config.toml")
    );
    Ok(())
}

#[test]
fn config_file_is_under_dot_config_on_macos_not_library() -> anyhow::Result<()> {
    let path = config_file_from(None, Some("/Users/ada"))?;
    assert_eq!(path, PathBuf::from("/Users/ada/.config/mike/config.toml"));
    assert!(!path.to_string_lossy().contains("Library"));
    Ok(())
}

#[test]
fn an_empty_or_relative_xdg_config_home_is_ignored() -> anyhow::Result<()> {
    for xdg in [Some(""), Some("relative/dir")] {
        assert_eq!(
            config_dir_from(xdg, Some("/home/ada"))?,
            PathBuf::from("/home/ada/.config/mike")
        );
    }
    Ok(())
}

#[test]
fn config_location_without_xdg_or_home_is_an_error_naming_both() {
    for home in [None, Some("")] {
        let error = config_file_from(None, home).err();
        assert!(matches!(error, Some(UserConfigError::NoConfigDir)));
        let text = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(
            text.contains("XDG_CONFIG_HOME") && text.contains("HOME"),
            "{text}"
        );
    }
}

#[test]
fn state_dir_shares_the_machine_identity_location() -> anyhow::Result<()> {
    assert_eq!(
        state_dir_from(Some("/srv/state"), Some("/home/ada"))?,
        PathBuf::from("/srv/state/mike")
    );
    assert_eq!(
        state_dir_from(None, Some("/Users/ada"))?,
        PathBuf::from("/Users/ada/.local/state/mike")
    );
    assert!(matches!(
        state_dir_from(None, None),
        Err(UserConfigError::NoStateDir)
    ));
    Ok(())
}
