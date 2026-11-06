use crate::db_provision::credentials::{
    Managed, generate_password, managed_from_url, save_database_url, stored_database_url, url_for,
};
use crate::db_provision::runner::sandbox_guard;

#[test]
fn a_url_built_by_mike_db_up_is_recognised_with_its_port_and_password() {
    let url = url_for("abc123", 6701);
    assert_eq!(url, "postgres://mike:abc123@127.0.0.1:6701/mike");
    assert_eq!(
        managed_from_url(&url),
        Some(Managed {
            port: 6701,
            password: "abc123".to_string()
        })
    );
}

#[test]
fn urls_pointing_anywhere_else_are_not_treated_as_managed() {
    for url in [
        "postgres://mike:pw@db.example:6666/mike",
        "postgres://ada:pw@127.0.0.1:6666/mike",
        "postgres://mike:pw@127.0.0.1:6666/other",
        "postgres://mike@127.0.0.1:6666/mike",
        "postgres://mike:pw@127.0.0.1/mike",
        "mysql://mike:pw@127.0.0.1:6666/mike",
        "not a url",
    ] {
        assert_eq!(managed_from_url(url), None, "{url}");
    }
}

#[test]
fn generated_passwords_are_long_hex_and_differ_every_time() {
    let first = generate_password();
    let second = generate_password();
    assert_eq!(first.len(), 64);
    assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(first, second);
}

#[test]
fn saving_a_url_writes_it_with_persistence_on_at_owner_only_mode() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("mike").join("config.toml");
    assert_eq!(stored_database_url(&file)?, None);
    save_database_url(&file, "postgres://mike:pw@127.0.0.1:6666/mike")?;
    assert_eq!(
        stored_database_url(&file)?.as_deref(),
        Some("postgres://mike:pw@127.0.0.1:6666/mike")
    );
    assert!(std::fs::read_to_string(&file)?.contains("persist_enabled = true"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file)?.permissions().mode() & 0o777,
            0o600
        );
    }
    Ok(())
}

#[test]
fn saving_into_a_file_that_is_not_toml_fails_and_leaves_it_alone() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "database_url = [unterminated")?;
    assert!(save_database_url(&file, "postgres://mike:pw@127.0.0.1:6666/mike").is_err());
    assert_eq!(
        std::fs::read_to_string(&file)?,
        "database_url = [unterminated"
    );
    Ok(())
}

#[test]
fn the_sandbox_guard_refuses_only_while_the_sandbox_is_on() {
    assert!(sandbox_guard(true).is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied));
    assert!(sandbox_guard(false).is_ok());
}
