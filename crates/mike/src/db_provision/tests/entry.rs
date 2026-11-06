use crate::db_provision::entry::first_failure;
use crate::doctor::{Check, Report};

#[test]
fn the_first_failed_check_becomes_the_message_for_the_setup_screen() {
    let report = Report::new(vec![
        Check::ok("database_url", "database url", "postgres://u:***@h/db"),
        Check::fail(
            "postgres_version",
            "postgres version",
            "PostgreSQL 17.9 is unsupported; mike requires PostgreSQL 19",
            None,
        ),
        Check::fail(
            "database_extensions",
            "database extensions",
            "missing",
            None,
        ),
    ]);
    let message = first_failure(&report).unwrap_or_default();
    assert!(
        message.starts_with("postgres version: PostgreSQL 17.9 is unsupported"),
        "{message}"
    );
    assert!(message.contains("mike db check"), "{message}");
}

#[test]
fn a_report_with_only_passes_and_warnings_has_no_failure_message() {
    let report = Report::new(vec![
        Check::ok("database_url", "database url", "postgres://u:***@h/db"),
        Check::warn(
            "database_migrations",
            "database migrations",
            "3 waiting",
            None,
        ),
    ]);
    assert_eq!(first_failure(&report), None);
}
