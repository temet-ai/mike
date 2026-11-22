use std::ffi::OsString;

use sqlx::migrate::Migrator;

pub(crate) const DIGEST: &str = env!("MIKE_MIGRATIONS_DIGEST");
const LIST_FLAG: &str = "--list-embedded-migrations";

pub(crate) fn migrator() -> Migrator {
    sqlx::migrate!("../mike-base/src/store/persistence/migrations")
}

pub(crate) fn list_requested(args: &[OsString]) -> bool {
    matches!(args, [only] if only == LIST_FLAG)
}

pub(crate) fn print_listing() {
    print!("{}", listing(&migrator()));
}

pub(crate) fn listing(migrator: &Migrator) -> String {
    let versions = migrator
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| format!("{}\n", migration.version));
    std::iter::once(format!("# digest {DIGEST}\n"))
        .chain(versions)
        .collect()
}

#[cfg(test)]
#[path = "embedded_migrations_tests.rs"]
mod embedded_migrations_tests;
