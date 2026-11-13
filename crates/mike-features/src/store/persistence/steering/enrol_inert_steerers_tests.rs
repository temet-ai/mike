use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistHandle, test_support};

const ENROL_INERT_STEERERS_MIGRATION: &str = include_str!(
    "../../../../../mike-base/src/store/persistence/migrations/20260921160000_enrol_inert_steerers.sql"
);

const AUTO_ENROLLED: [&str; 3] = [
    "ast-grep-over-grep",
    "monitor-over-sleep",
    "tee-then-slice-gates",
];
const UNTOUCHED: [&str; 3] = ["verifier", "no-cat-in-bash", "no-full-spill-read"];

type SteererFlags = (String, bool, bool);
type SteererStamp = (String, DateTime<Utc>);

async fn flags_by_name(pool: &sqlx::PgPool) -> anyhow::Result<Vec<SteererFlags>> {
    let rows = sqlx::query_as::<_, SteererFlags>(
        "SELECT name::text, enabled, auto_enrol FROM steering.steerers \
         WHERE deleted_at IS NULL ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn stamps_by_name(pool: &sqlx::PgPool) -> anyhow::Result<Vec<SteererStamp>> {
    let rows = sqlx::query_as::<_, SteererStamp>(
        "SELECT name::text, updated_at FROM steering.steerers ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn flags_of(rows: &[SteererFlags], name: &str) -> anyhow::Result<(bool, bool)> {
    rows.iter()
        .find(|(n, _, _)| n == name)
        .map(|(_, enabled, auto_enrol)| (*enabled, *auto_enrol))
        .ok_or_else(|| anyhow::anyhow!("steerer {name:?} missing from steering.steerers"))
}

fn assert_migrated(rows: &[SteererFlags]) -> anyhow::Result<()> {
    for name in AUTO_ENROLLED {
        assert_eq!(
            flags_of(rows, name)?,
            (true, true),
            "{name}: must stay enabled and auto-enrol so sessions evaluate it"
        );
    }
    assert_eq!(
        flags_of(rows, "memory-keeper")?,
        (false, false),
        "memory-keeper has no code and must be disabled"
    );
    Ok(())
}

fn assert_others_untouched(before: &[SteererFlags], after: &[SteererFlags]) -> anyhow::Result<()> {
    for name in UNTOUCHED {
        assert_eq!(
            flags_of(before, name)?,
            flags_of(after, name)?,
            "{name}: the migration must not touch other steerers"
        );
    }
    Ok(())
}

async fn restore_pre_migration_state(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("UPDATE steering.steerers SET auto_enrol = false WHERE name = ANY($1)")
        .bind(AUTO_ENROLLED.map(str::to_owned).to_vec())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE steering.steerers SET enabled = true WHERE name = 'memory-keeper'")
        .execute(pool)
        .await?;
    Ok(())
}

async fn apply_migration(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::raw_sql(ENROL_INERT_STEERERS_MIGRATION)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn enrol_inert_steerers_migration_leaves_fresh_database_enrolled() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    assert_migrated(&flags_by_name(handle.pool()).await?)?;

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn enrol_inert_steerers_migration_repairs_live_state_and_converges() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();

    restore_pre_migration_state(pool).await?;
    let before = flags_by_name(pool).await?;
    assert_eq!(flags_of(&before, "ast-grep-over-grep")?, (true, false));
    assert_eq!(flags_of(&before, "memory-keeper")?, (true, false));

    apply_migration(pool).await?;
    let after = flags_by_name(pool).await?;
    assert_migrated(&after)?;
    assert_others_untouched(&before, &after)?;

    let stamps = stamps_by_name(pool).await?;
    apply_migration(pool).await?;
    assert_eq!(
        stamps,
        stamps_by_name(pool).await?,
        "a second application must change no row"
    );

    db.cleanup().await?;
    Ok(())
}
