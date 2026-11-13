use sqlx::AssertSqlSafe;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};

use super::url::{extract_db_name, extract_host_port, replace_db_name};
use crate::store::persistence::migrations::installed_migrator;

static TEMPLATE_DB_NAME: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

async fn template_db_name() -> anyhow::Result<&'static str> {
    let name = TEMPLATE_DB_NAME
        .get_or_try_init(|| async {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            for m in installed_migrator().await?.iter() {
                hasher.update(m.version.to_le_bytes());
                hasher.update(&m.checksum);
            }
            let digest = hex::encode(&hasher.finalize()[..6]);
            anyhow::Ok(format!("claude_proxy_test_template_{digest}"))
        })
        .await?;
    Ok(name.as_str())
}

const DEV_DB_NAME: &str = "claude_proxy";

const RUN_DB_ADVISORY_KEY: i64 = 0x7072_7879_7464_6273;

const TEMPLATE_DB_ADVISORY_KEY: i64 = 0x7072_7879_7464_746d;

const CAPTURE_INSTANCE_PORT: &str = "5440";

const SWEEP_MIN_AGE_MINUTES: i32 = 60;

const SWEEP_NAME_PREFIX: &str = "claude_proxy_test_";

pub fn guard_against_dev_db(admin_url: &str) -> anyhow::Result<()> {
    let db = extract_db_name(admin_url)?;
    if db == DEV_DB_NAME {
        anyhow::bail!(
            "MIKE_TEST_ADMIN_URL points at '{DEV_DB_NAME}' — the dev database. \
             Point it at 'postgres' (the admin DB) instead; tests will provision per-test clones."
        );
    }
    let host_port = extract_host_port(admin_url)?;
    if host_port.rsplit(':').next() == Some(CAPTURE_INSTANCE_PORT) {
        anyhow::bail!(
            "MIKE_TEST_ADMIN_URL points at {host_port}, the live capture instance (mike#314). \
             Tests never run against it; start the throwaway instance with scripts/test-db/up.sh \
             and point MIKE_TEST_ADMIN_URL at 127.0.0.1 port 6667."
        );
    }
    Ok(())
}

pub async fn guard_against_capture_instance(admin_url: &str) -> anyhow::Result<()> {
    refuse_instance_hosting(admin_url, DEV_DB_NAME).await
}

async fn refuse_instance_hosting(admin_url: &str, marker_db: &str) -> anyhow::Result<()> {
    let admin_pool = connect_single(admin_url).await?;
    let hosts_marker = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1) AS "exists!""#,
        marker_db,
    )
    .fetch_one(&admin_pool)
    .await?;
    admin_pool.close().await;
    if hosts_marker {
        anyhow::bail!(
            "the instance behind MIKE_TEST_ADMIN_URL hosts the '{marker_db}' capture database \
             (mike#314). Tests never provision databases on it; use the throwaway instance from \
             scripts/test-db/up.sh."
        );
    }
    Ok(())
}

pub async fn prepare_template_db(admin_url: &str) -> anyhow::Result<()> {
    let template_name = template_db_name().await?;
    let migrator = installed_migrator().await?;
    let admin_pool = connect_single(admin_url).await?;

    sqlx::query!("SELECT pg_advisory_lock($1)", TEMPLATE_DB_ADVISORY_KEY)
        .fetch_one(&admin_pool)
        .await?;

    let result = async {
        let exists = sqlx::query_scalar!(
            "SELECT datname::text FROM pg_database WHERE datname = $1",
            template_name,
        )
        .fetch_optional(&admin_pool)
        .await?;
        if exists.is_none() {
            let stmt = format!("CREATE DATABASE \"{template_name}\"");
            ignore_duplicate_database(admin_pool.execute(AssertSqlSafe(stmt)).await.map(|_| ()))?;
        }

        let template_url = replace_db_name(admin_url, template_name)?;
        let template_pool = connect_single(&template_url).await?;
        migrator.run(&template_pool).await?;
        template_pool.close().await;
        anyhow::Ok(())
    }
    .await;

    let _ = sqlx::query!("SELECT pg_advisory_unlock($1)", TEMPLATE_DB_ADVISORY_KEY)
        .fetch_one(&admin_pool)
        .await;
    admin_pool.close().await;
    result
}

fn is_missing_template<T>(res: &Result<T, sqlx::Error>) -> bool {
    match res {
        Err(sqlx::Error::Database(e)) => {
            let m = e.message();
            m.contains("template database") && m.contains("does not exist")
        }
        _ => false,
    }
}

pub async fn create_database_from_template(admin_url: &str, db_name: &str) -> anyhow::Result<()> {
    let template_name = template_db_name().await?;
    let admin_pool = connect_single(admin_url).await?;
    let stmt =
        format!("CREATE DATABASE \"{db_name}\" TEMPLATE \"{template_name}\" STRATEGY FILE_COPY");
    let mut res = admin_pool
        .execute(AssertSqlSafe(stmt.clone()))
        .await
        .map(|_| ());
    if is_missing_template(&res) {
        prepare_template_db(admin_url).await?;
        res = admin_pool
            .execute(AssertSqlSafe(stmt.clone()))
            .await
            .map(|_| ());
    }
    admin_pool.close().await;
    ignore_duplicate_database(res)
}

pub fn run_db_name() -> String {
    let run_id = std::env::var("NEXTEST_RUN_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("proc{}", std::process::id()));
    let sanitised: String = run_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("claude_proxy_test_run_{sanitised}")
}

pub async fn ensure_run_db(admin_url: &str, run_db: &str) -> anyhow::Result<()> {
    let admin_pool = connect_single(admin_url).await?;
    sqlx::query!("SELECT pg_advisory_lock($1)", RUN_DB_ADVISORY_KEY)
        .fetch_one(&admin_pool)
        .await?;

    let result = async {
        let exists = sqlx::query_scalar!(
            "SELECT datname::text FROM pg_database WHERE datname = $1",
            run_db,
        )
        .fetch_optional(&admin_pool)
        .await?;
        if exists.is_none() {
            sweep_orphan_test_dbs(&admin_pool, run_db).await?;
            let template_name = template_db_name().await?;
            let stmt = format!(
                "CREATE DATABASE \"{run_db}\" TEMPLATE \"{template_name}\" STRATEGY FILE_COPY"
            );
            let mut res = admin_pool
                .execute(AssertSqlSafe(stmt.clone()))
                .await
                .map(|_| ());
            if is_missing_template(&res) {
                prepare_template_db(admin_url).await?;
                res = admin_pool
                    .execute(AssertSqlSafe(stmt.clone()))
                    .await
                    .map(|_| ());
            }
            ignore_duplicate_database(res)?;
        }
        anyhow::Ok(())
    }
    .await;

    let _ = sqlx::query!("SELECT pg_advisory_unlock($1)", RUN_DB_ADVISORY_KEY)
        .fetch_one(&admin_pool)
        .await;
    admin_pool.close().await;
    result
}

async fn sweep_orphan_test_dbs(admin_pool: &PgPool, keep_run_db: &str) -> anyhow::Result<()> {
    sweep_orphans(
        admin_pool,
        SWEEP_NAME_PREFIX,
        keep_run_db,
        SWEEP_MIN_AGE_MINUTES,
    )
    .await
}

async fn sweep_orphans(
    admin_pool: &PgPool,
    name_prefix: &str,
    keep_run_db: &str,
    min_age_minutes: i32,
) -> anyhow::Result<()> {
    let orphans = sqlx::query_scalar!(
        r#"SELECT d.datname::text AS "datname!" FROM pg_database d
          WHERE starts_with(d.datname, $1)
            AND d.datname NOT LIKE 'claude_proxy_test_template%'
            AND d.datname <> $2
            AND NOT EXISTS (SELECT 1 FROM pg_stat_activity a WHERE a.datname = d.datname)
            AND (pg_stat_file('base/' || d.oid || '/PG_VERSION', true)).modification
                < now() - make_interval(mins => $3)"#,
        name_prefix,
        keep_run_db,
        min_age_minutes,
    )
    .fetch_all(admin_pool)
    .await?;
    for name in orphans {
        let _ = admin_pool
            .execute(AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{name}\"")))
            .await;
    }
    Ok(())
}

fn ignore_duplicate_database(res: Result<(), sqlx::Error>) -> anyhow::Result<()> {
    match res {
        Ok(()) => Ok(()),
        Err(sqlx::Error::Database(db))
            if matches!(db.code().as_deref(), Some("42P04") | Some("23505")) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn drop_database(admin_url: &str, db_name: &str) -> anyhow::Result<()> {
    let admin_pool = connect_single(admin_url).await?;
    let terminate = format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{db_name}' AND pid <> pg_backend_pid()"
    );
    let _ = admin_pool.execute(AssertSqlSafe(terminate)).await;
    let drop_stmt = format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)");
    admin_pool.execute(AssertSqlSafe(drop_stmt)).await?;
    admin_pool.close().await;
    Ok(())
}

async fn connect_single(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
    Ok(pool)
}

#[cfg(test)]
#[path = "provisioning_tests.rs"]
mod tests;
