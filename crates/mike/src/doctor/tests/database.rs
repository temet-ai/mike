use super::*;

#[test]
fn a_missing_database_url_with_persistence_on_fails_and_points_at_the_database_step()
-> anyhow::Result<()> {
    let mut observed = healthy();
    observed.database.redacted_url = None;
    observed.database.server = None;
    let checks = evaluate(&observed);
    let url = check(&checks, "database_url")?;
    assert_eq!(url.status, Status::Fail);
    assert_eq!(url.fix.as_deref(), Some("mike setup database"));
    assert!(checks.iter().all(|c| c.id != "database_reachable"));
    Ok(())
}

#[test]
fn switching_persistence_off_is_a_warning_not_a_failure() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.database.persist_enabled = false;
    observed.database.redacted_url = None;
    observed.database.server = None;
    assert_eq!(
        check(&evaluate(&observed), "database_url")?.status,
        Status::Warn
    );
    Ok(())
}

#[test]
fn an_unreachable_database_fails_with_the_error_text() -> anyhow::Result<()> {
    let mut observed = healthy();
    observed.database.server = Some(Err("connection refused".to_string()));
    let checks = evaluate(&observed);
    let reach = check(&checks, "database_reachable")?;
    assert_eq!(reach.status, Status::Fail);
    assert!(
        reach.detail.contains("connection refused"),
        "{}",
        reach.detail
    );
    assert!(checks.iter().all(|c| c.id != "postgres_version"));
    Ok(())
}

#[test]
fn postgres_19_is_the_only_supported_major() -> anyhow::Result<()> {
    let mut observed = healthy();
    for major in [15, 16, 17, 18, 20] {
        server_mut(&mut observed)?.version_num = major * 10_000;
        let checks = evaluate(&observed);
        let version = check(&checks, "postgres_version")?;
        assert_eq!(version.status, Status::Fail);
        assert!(version.detail.contains("requires PostgreSQL 19"));
    }
    for version in [190_000, 190_001] {
        server_mut(&mut observed)?.version_num = version;
        assert_eq!(
            check(&evaluate(&observed), "postgres_version")?.status,
            Status::Ok
        );
    }
    Ok(())
}

#[test]
fn an_extension_the_server_does_not_provide_fails() -> anyhow::Result<()> {
    let mut observed = healthy();
    for extension in &mut server_mut(&mut observed)?.extensions {
        if extension.name == "vector" {
            extension.installed = false;
            extension.available = false;
        }
    }
    let checks = evaluate(&observed);
    let extensions = check(&checks, "database_extensions")?;
    assert_eq!(extensions.status, Status::Fail);
    assert!(
        extensions.detail.contains("vector"),
        "{}",
        extensions.detail
    );
    Ok(())
}

#[test]
fn an_extension_the_user_may_not_create_fails_and_names_it_and_a_superuser() -> anyhow::Result<()> {
    let mut observed = healthy();
    for extension in &mut server_mut(&mut observed)?.extensions {
        extension.installed = extension.name != "vector";
    }
    assert_eq!(
        check(&evaluate(&observed), "database_extensions")?.status,
        Status::Ok
    );
    for extension in &mut server_mut(&mut observed)?.extensions {
        extension.creatable = extension.name != "vector";
    }
    let checks = evaluate(&observed);
    let extensions = check(&checks, "database_extensions")?;
    assert_eq!(extensions.status, Status::Fail);
    assert!(
        extensions.detail.contains("superuser") && extensions.detail.contains("create vector"),
        "{}",
        extensions.detail
    );
    Ok(())
}

#[test]
fn trusted_extensions_are_creatable_without_a_superuser() -> anyhow::Result<()> {
    let mut observed = healthy();
    for extension in &mut server_mut(&mut observed)?.extensions {
        extension.installed = extension.name == "vector";
    }
    assert_eq!(
        check(&evaluate(&observed), "database_extensions")?.status,
        Status::Ok
    );
    Ok(())
}

#[test]
fn migrations_pending_warn_and_blocking_ones_fail() -> anyhow::Result<()> {
    let mut observed = healthy();
    server_mut(&mut observed)?.migrations = Ok(MigrationFacts {
        pending: 3,
        blocking: Vec::new(),
    });
    let pending = evaluate(&observed);
    assert_eq!(check(&pending, "database_migrations")?.status, Status::Warn);
    server_mut(&mut observed)?.migrations = Ok(MigrationFacts {
        pending: 0,
        blocking: vec!["1 applied migrations were changed after they ran".to_string()],
    });
    assert_eq!(
        check(&evaluate(&observed), "database_migrations")?.status,
        Status::Fail
    );
    server_mut(&mut observed)?.migrations = Err("no table".to_string());
    assert_eq!(
        check(&evaluate(&observed), "database_migrations")?.status,
        Status::Fail
    );
    Ok(())
}
