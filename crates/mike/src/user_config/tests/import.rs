use super::*;

#[test]
fn dotenv_parsing_reads_values_literally() {
    let parsed = parse_dotenv(
        "# comment\n\nMIKE_A=plain\nexport MIKE_B='single $x'\nMIKE_C=\"double ${y}\"\nMIKE_D=pa$$word # trailing\nnot an assignment\n=oops\nMIKE_E=\n",
    );
    assert_eq!(
        parsed,
        [
            ("MIKE_A".to_string(), "plain".to_string()),
            ("MIKE_B".to_string(), "single $x".to_string()),
            ("MIKE_C".to_string(), "double ${y}".to_string()),
            ("MIKE_D".to_string(), "pa$$word".to_string()),
            ("MIKE_E".to_string(), String::new()),
        ]
    );
}

#[test]
fn import_copies_only_proxy_values_and_never_overwrites_a_key_the_file_has() -> anyhow::Result<()> {
    let (_dir, path) =
        temp_file("listen_addr = \"127.0.0.1:1\"\n[embedding]\nurl = \"http://mine\"\n")?;
    let report = import_env(
        "MIKE_LISTEN_ADDR=127.0.0.1:2\nMIKE_EMBEDDING__URL=http://theirs\nMIKE_DATABASE_URL=postgres://u:p$w@h/db\nHOME=/x\nPATH=/bin\n",
        &path,
    )?;
    assert_eq!(report.imported, ["database_url".to_string()]);
    assert_eq!(
        report.kept,
        ["listen_addr".to_string(), "embedding__url".to_string()]
    );
    assert_eq!(report.ignored, 2);
    let entries = ConfigFile::open(&path)?.entries()?;
    let find = |name: &str| entries.iter().find(|e| e.env_name == name).cloned();
    assert_eq!(
        find("MIKE_LISTEN_ADDR"),
        Some(literal("MIKE_LISTEN_ADDR", "127.0.0.1:1"))
    );
    assert_eq!(
        find("MIKE_EMBEDDING__URL"),
        Some(literal("MIKE_EMBEDDING__URL", "http://mine"))
    );
    assert_eq!(
        find("MIKE_DATABASE_URL"),
        Some(literal("MIKE_DATABASE_URL", "postgres://u:p$w@h/db"))
    );
    Ok(())
}

#[test]
fn importing_twice_changes_nothing_the_second_time() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let first = import_env("MIKE_LAUNCH_BACKEND=herdr\n", &path)?;
    assert_eq!(first.imported.len(), 1);
    let before = std::fs::read_to_string(&path)?;
    let second = import_env("MIKE_LAUNCH_BACKEND=tmux\n", &path)?;
    assert!(second.imported.is_empty());
    assert_eq!(second.kept, ["launch_backend".to_string()]);
    assert_eq!(std::fs::read_to_string(&path)?, before);
    Ok(())
}

#[test]
fn importing_nothing_does_not_create_the_config_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mike").join("config.toml");
    let report = import_env("HOME=/x\n", &path)?;
    assert!(report.imported.is_empty());
    assert!(!path.exists());
    Ok(())
}

#[test]
fn importing_into_a_broken_config_file_fails_without_touching_it() -> anyhow::Result<()> {
    let (_dir, path) = temp_file("database_url = \n")?;
    assert!(import_env("MIKE_LAUNCH_BACKEND=herdr\n", &path).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "database_url = \n");
    Ok(())
}
