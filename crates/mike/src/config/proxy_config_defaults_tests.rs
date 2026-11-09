use super::*;

#[test]
fn for_tests_in_memory_when_called_twice_uses_distinct_existing_folders() -> anyhow::Result<()> {
    let first = ProxyConfig::for_tests_in_memory().config_path;
    let second = ProxyConfig::for_tests_in_memory().config_path;

    let first_folder = first
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config path has no folder: {}", first.display()))?;
    let second_folder = second
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config path has no folder: {}", second.display()))?;
    assert_ne!(first_folder, second_folder);
    assert!(first_folder.is_dir(), "{}", first_folder.display());
    assert!(second_folder.is_dir(), "{}", second_folder.display());
    std::fs::remove_dir(first_folder)?;
    std::fs::remove_dir(second_folder)?;
    Ok(())
}

#[test]
fn for_tests_in_memory_when_called_keeps_the_folder_path_short_enough_for_sockets()
-> anyhow::Result<()> {
    let config_path = ProxyConfig::for_tests_in_memory().config_path;

    let folder = config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config path has no folder: {}", config_path.display()))?;
    assert!(
        folder.as_os_str().len() <= 40,
        "{} is {} bytes",
        folder.display(),
        folder.as_os_str().len()
    );
    std::fs::remove_dir(folder)?;
    Ok(())
}
