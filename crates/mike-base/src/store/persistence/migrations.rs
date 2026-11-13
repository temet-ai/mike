use std::collections::HashMap;
use std::sync::LazyLock;

use sqlx::migrate::Migrator;
use sqlx::{PgConnection, PgPool};

use super::error::PersistError;
use super::pool;

static MIGRATOR: Migrator = sqlx::migrate!("src/store/persistence/migrations");

static SERVE_MIGRATOR: LazyLock<Migrator> = LazyLock::new(|| {
    let mut migrator = sqlx::migrate!("src/store/persistence/migrations");
    migrator.set_ignore_missing(true);
    migrator
});

async fn warn_on_newer_migrations(conn: &mut PgConnection) {
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
            .fetch_all(&mut *conn)
            .await
            .unwrap_or_default();
    let newer: Vec<i64> = applied
        .into_iter()
        .filter(|version| !MIGRATOR.migrations.iter().any(|m| m.version == *version))
        .collect();
    if !newer.is_empty() {
        tracing::warn!(
            versions = ?newer,
            "the database has migrations from a newer mike; serving with this binary's schema until it is deployed"
        );
    }
}

pub async fn run(pool: &PgPool) -> Result<(), PersistError> {
    pool::require_server_version(pool).await?;
    MIGRATOR.run(pool).await?;
    Ok(())
}

pub async fn run_patient(database_url: &str) -> Result<(), PersistError> {
    let lock_timeout_secs = pool::migration_lock_timeout_secs();
    tracing::info!(
        lock_timeout_secs,
        "running migrations on a dedicated connection with a patient lock timeout"
    );
    let mut conn = pool::build_migration_connection(database_url).await?;
    pool::require_server_version(&mut conn).await?;
    warn_on_newer_migrations(&mut conn).await;
    SERVE_MIGRATOR.run(&mut conn).await?;
    Ok(())
}

/// Refuses unless every embedded migration is already applied and the ledger
/// is compatible with the binary. Applies nothing.
pub async fn require_current(pool: &PgPool) -> Result<(), PersistError> {
    let assessment = assess(pool).await?;
    match assessment.not_current_reason() {
        Some(reason) => Err(PersistError::SchemaNotCurrent(reason)),
        None => Ok(()),
    }
}

#[derive(Debug, Clone)]
pub struct MismatchedMigration {
    pub version: i64,
    pub description: String,
    pub binary_checksum: String,
    pub db_checksum: String,
}

#[derive(Debug, Clone)]
pub struct OutOfOrderMigration {
    pub version: i64,
    pub description: String,
    pub max_applied: i64,
}

#[derive(Debug)]
pub struct MigrationAssessment {
    pub mismatched: Vec<MismatchedMigration>,

    pub missing_from_binary: Vec<i64>,

    pub pending: Vec<i64>,

    pub out_of_order: Vec<OutOfOrderMigration>,
}

impl MigrationAssessment {
    pub fn is_blocking(&self) -> bool {
        !self.mismatched.is_empty()
            || !self.missing_from_binary.is_empty()
            || !self.out_of_order.is_empty()
    }

    /// Why a non-migrating connection must refuse, or `None` when the ledger
    /// matches the binary exactly.
    pub fn not_current_reason(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.pending.is_empty() {
            parts.push(format!(
                "{} pending: {:?}",
                self.pending.len(),
                self.pending
            ));
        }
        if !self.mismatched.is_empty() {
            let versions: Vec<i64> = self.mismatched.iter().map(|m| m.version).collect();
            parts.push(format!(
                "{} checksum-mismatched: {versions:?}",
                versions.len()
            ));
        }
        if !self.missing_from_binary.is_empty() {
            parts.push(format!(
                "{} applied but absent from this binary: {:?}",
                self.missing_from_binary.len(),
                self.missing_from_binary
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

fn hex_prefix(bytes: &[u8], n: usize) -> String {
    bytes.iter().take(n).map(|b| format!("{b:02x}")).collect()
}

fn detect_out_of_order(
    pending: &[(i64, String)],
    max_applied: Option<i64>,
) -> Vec<OutOfOrderMigration> {
    let Some(max_applied) = max_applied else {
        return Vec::new();
    };
    pending
        .iter()
        .filter(|(version, _)| *version < max_applied)
        .map(|(version, description)| OutOfOrderMigration {
            version: *version,
            description: description.clone(),
            max_applied,
        })
        .collect()
}

pub async fn assess(pool: &PgPool) -> Result<MigrationAssessment, PersistError> {
    let table_exists: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations')::text")
            .fetch_one(pool)
            .await?;

    let embedded: HashMap<i64, (&[u8], &str)> = MIGRATOR
        .migrations
        .iter()
        .map(|m| (m.version, (m.checksum.as_ref(), m.description.as_ref())))
        .collect();

    if table_exists.is_none() {
        let pending = {
            let mut v: Vec<i64> = embedded.keys().copied().collect();
            v.sort_unstable();
            v
        };
        return Ok(MigrationAssessment {
            mismatched: vec![],
            missing_from_binary: vec![],
            pending,
            out_of_order: vec![],
        });
    }

    let stored_rows =
        sqlx::query!("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.version, row.checksum, row.success))
            .collect();

    let stored = successful_checksums(stored_rows)?;
    let max_applied = stored.keys().max().copied();

    let mut mismatched = Vec::new();
    let mut missing_from_binary = Vec::new();
    let mut pending = Vec::new();
    let mut pending_with_desc: Vec<(i64, String)> = Vec::new();

    for (&version, &(on_disk, description)) in &embedded {
        match stored.get(&version) {
            Some(in_db) if in_db.as_slice() != on_disk => {
                mismatched.push(MismatchedMigration {
                    version,
                    description: description.to_owned(),
                    binary_checksum: hex_prefix(on_disk, 4),
                    db_checksum: hex_prefix(in_db, 4),
                });
            }
            Some(_) => {}
            None => {
                pending.push(version);
                pending_with_desc.push((version, description.to_owned()));
            }
        }
    }

    for &version in stored.keys() {
        if !embedded.contains_key(&version) {
            missing_from_binary.push(version);
        }
    }

    let mut out_of_order = detect_out_of_order(&pending_with_desc, max_applied);

    mismatched.sort_unstable_by_key(|m| m.version);
    missing_from_binary.sort_unstable();
    pending.sort_unstable();
    out_of_order.sort_unstable_by_key(|m| m.version);

    Ok(MigrationAssessment {
        mismatched,
        missing_from_binary,
        pending,
        out_of_order,
    })
}

fn successful_checksums(
    rows: Vec<(i64, Vec<u8>, bool)>,
) -> Result<HashMap<i64, Vec<u8>>, PersistError> {
    if let Some((version, _, _)) = rows.iter().find(|(_, _, success)| !success) {
        return Err(PersistError::SchemaNotCurrent(format!(
            "failed migration {version}; resolve it with the matching binary before continuing; no history was changed"
        )));
    }
    Ok(rows
        .into_iter()
        .map(|(version, checksum, _)| (version, checksum))
        .collect())
}

#[cfg(test)]
pub fn migration_count() -> usize {
    MIGRATOR.iter().count()
}

#[cfg(test)]
#[path = "migrations_newer_tests.rs"]
pub mod newer_tests;

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn failed_ledger_row_is_never_treated_as_applied() -> anyhow::Result<()> {
        let error = successful_checksums(vec![(1, vec![1], true), (2, vec![2], false)])
            .err()
            .ok_or_else(|| anyhow::anyhow!("failed migration was accepted"))?;
        let PersistError::SchemaNotCurrent(reason) = error else {
            anyhow::bail!("failed migration must be classified as schema not current");
        };
        assert!(reason.contains("failed migration 2"), "{reason}");
        Ok(())
    }

    #[test]
    fn successful_ledger_rows_preserve_versions_and_checksums() -> anyhow::Result<()> {
        let stored = successful_checksums(vec![(1, vec![1], true), (2, vec![2], true)])?;
        assert_eq!(stored, HashMap::from([(1, vec![1]), (2, vec![2])]));
        assert!(successful_checksums(Vec::new())?.is_empty());
        Ok(())
    }

    #[test]
    fn migrator_has_at_least_one_migration() {
        assert!(
            migration_count() >= 1,
            "expected at least the initial schema migration to be embedded"
        );
    }

    fn assessment(pending: Vec<i64>, missing_from_binary: Vec<i64>) -> MigrationAssessment {
        MigrationAssessment {
            mismatched: vec![],
            missing_from_binary,
            pending,
            out_of_order: vec![],
        }
    }

    #[test]
    fn not_current_reason_is_none_when_ledger_matches_binary() {
        assert_eq!(assessment(vec![], vec![]).not_current_reason(), None);
    }

    #[test]
    fn not_current_reason_names_pending_versions() {
        let reason = assessment(vec![20260924100000], vec![]).not_current_reason();
        let reason = reason.unwrap_or_default();
        assert!(reason.contains("1 pending"), "{reason}");
        assert!(reason.contains("20260924100000"), "{reason}");
    }

    #[test]
    fn not_current_reason_names_versions_absent_from_binary() {
        let reason = assessment(vec![], vec![20260930000000]).not_current_reason();
        let reason = reason.unwrap_or_default();
        assert!(reason.contains("absent from this binary"), "{reason}");
        assert!(reason.contains("20260930000000"), "{reason}");
    }

    fn v(version: i64, description: &str) -> (i64, String) {
        (version, description.to_owned())
    }

    #[test]
    fn detect_out_of_order_flags_pending_below_max_applied() {
        let pending = vec![v(20260712000002, "efficiency_steerer_seeds")];
        let out_of_order = detect_out_of_order(&pending, Some(20260712000003));

        assert_eq!(out_of_order.len(), 1);
        assert_eq!(out_of_order[0].version, 20260712000002);
        assert_eq!(out_of_order[0].description, "efficiency_steerer_seeds");
        assert_eq!(out_of_order[0].max_applied, 20260712000003);
    }

    #[test]
    fn detect_out_of_order_does_not_flag_pending_above_max_applied() {
        let pending = vec![v(20260712000004, "later_migration")];
        let out_of_order = detect_out_of_order(&pending, Some(20260712000003));

        assert!(
            out_of_order.is_empty(),
            "a pending migration newer than max_applied is normal, not out-of-order: {out_of_order:?}"
        );
    }

    #[test]
    fn detect_out_of_order_is_empty_when_nothing_applied_yet() {
        let pending = vec![v(20260712000001, "first"), v(20260712000002, "second")];
        let out_of_order = detect_out_of_order(&pending, None);

        assert!(
            out_of_order.is_empty(),
            "an empty/fresh database has no max-applied version, so nothing can be out of \
             order: {out_of_order:?}"
        );
    }
}
