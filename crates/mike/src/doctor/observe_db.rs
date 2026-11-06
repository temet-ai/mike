use std::time::Duration;

use sqlx::PgPool;

use super::observed::{DatabaseFacts, ExtensionFacts, MigrationFacts, ServerFacts};
use crate::config::ProxyConfig;
use crate::store::persistence::{migrations, pool};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const REQUIRED_EXTENSIONS: [&str; 3] = ["vector", "pgcrypto", "pg_trgm"];

pub(super) async fn observe_database(config: &ProxyConfig) -> DatabaseFacts {
    let url = config
        .database_url
        .as_deref()
        .filter(|u| !u.trim().is_empty());
    let server = match url {
        Some(url) if config.persist_enabled => Some(inspect_server(url).await),
        _ => None,
    };
    DatabaseFacts {
        persist_enabled: config.persist_enabled,
        redacted_url: url.map(crate::config::load::redact_database_url),
        server,
    }
}

pub(super) async fn inspect_server(url: &str) -> Result<ServerFacts, String> {
    let work = async {
        let pool = pool::build_read_only_pool(url)
            .await
            .map_err(|e| e.to_string())?;
        let facts = read_server_facts(&pool).await;
        pool.close().await;
        facts
    };
    tokio::time::timeout(CONNECT_TIMEOUT, work)
        .await
        .unwrap_or_else(|_| Err(format!("no answer within {}s", CONNECT_TIMEOUT.as_secs())))
}

async fn read_server_facts(pool: &PgPool) -> Result<ServerFacts, String> {
    let version_num: String = sqlx::query_scalar("SHOW server_version_num")
        .fetch_one(pool)
        .await
        .map_err(|e| format!("could not read the server version: {e}"))?;
    let version_num = version_num
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("unreadable server version `{version_num}`: {e}"))?;
    let names: Vec<String> = REQUIRED_EXTENSIONS
        .iter()
        .map(ToString::to_string)
        .collect();
    let installed: Vec<String> =
        sqlx::query_scalar("SELECT extname::text FROM pg_extension WHERE extname = ANY($1)")
            .bind(&names)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("could not read installed extensions: {e}"))?;
    let available: Vec<String> =
        sqlx::query_scalar("SELECT name::text FROM pg_available_extensions WHERE name = ANY($1)")
            .bind(&names)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("could not read available extensions: {e}"))?;
    let is_superuser: bool =
        sqlx::query_scalar("SELECT rolsuper FROM pg_roles WHERE rolname = current_user")
            .fetch_one(pool)
            .await
            .unwrap_or(false);
    let user_creatable = user_creatable_extensions(pool, &names).await?;
    Ok(ServerFacts {
        version_num,
        extensions: REQUIRED_EXTENSIONS
            .iter()
            .map(|&name| ExtensionFacts {
                name,
                installed: installed.iter().any(|n| n == name),
                available: available.iter().any(|n| n == name),
                creatable: is_superuser || user_creatable.iter().any(|n| n == name),
            })
            .collect(),
        migrations: migration_facts(pool).await,
    })
}

async fn user_creatable_extensions(pool: &PgPool, names: &[String]) -> Result<Vec<String>, String> {
    let may_create_in_database: bool =
        sqlx::query_scalar("SELECT has_database_privilege(current_database(), 'CREATE')")
            .fetch_one(pool)
            .await
            .map_err(|e| format!("could not read the database privileges: {e}"))?;
    if !may_create_in_database {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT v.name::text \
           FROM pg_available_extension_versions v \
           JOIN pg_available_extensions a \
             ON a.name = v.name AND a.default_version = v.version \
          WHERE v.name = ANY($1) AND (NOT v.superuser OR v.trusted)",
    )
    .bind(names)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("could not read which extensions are trusted: {e}"))
}

async fn migration_facts(pool: &PgPool) -> Result<MigrationFacts, String> {
    let assessment = migrations::assess(pool).await.map_err(|e| e.to_string())?;
    let mut blocking = Vec::new();
    if !assessment.mismatched.is_empty() {
        blocking.push(format!(
            "{} applied migrations were changed after they ran",
            assessment.mismatched.len()
        ));
    }
    if !assessment.missing_from_binary.is_empty() {
        blocking.push(format!(
            "{} migrations in the database are unknown to this build; install a newer mike",
            assessment.missing_from_binary.len()
        ));
    }
    if !assessment.out_of_order.is_empty() {
        blocking.push(format!(
            "{} pending migrations sort below an applied one",
            assessment.out_of_order.len()
        ));
    }
    Ok(MigrationFacts {
        pending: assessment.pending.len(),
        blocking,
    })
}
