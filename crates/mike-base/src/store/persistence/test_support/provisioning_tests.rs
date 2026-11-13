use super::*;

fn required_admin_url() -> anyhow::Result<Option<String>> {
    let admin_url = super::super::requirements::admin_url()?;
    guard_against_dev_db(&admin_url)?;
    Ok(Some(admin_url))
}

fn unique_prefix(case: &str) -> String {
    format!(
        "claude_proxy_test_{case}_{}_",
        uuid::Uuid::new_v4().simple()
    )
}

async fn create_empty_db(admin_pool: &PgPool, name: &str) -> anyhow::Result<()> {
    admin_pool
        .execute(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
        .await?;
    Ok(())
}

async fn db_exists(admin_pool: &PgPool, name: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(name)
            .fetch_one(admin_pool)
            .await?,
    )
}

async fn force_drop(admin_pool: &PgPool, name: &str) {
    let _ = admin_pool
        .execute(AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
        )))
        .await;
}

#[test]
fn guard_rejects_dev_db_by_name() -> anyhow::Result<()> {
    let err = match guard_against_dev_db("postgres://proxy:proxy@localhost:6666/claude_proxy") {
        Ok(()) => anyhow::bail!("guard_against_dev_db must reject the dev DB name"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(msg.contains("dev database"), "got: {msg}");
    assert!(msg.contains("claude_proxy"), "got: {msg}");
    Ok(())
}

#[test]
fn guard_accepts_admin_db() -> anyhow::Result<()> {
    guard_against_dev_db("postgres://proxy:proxy@localhost:6666/postgres")?;
    Ok(())
}

#[test]
fn guard_rejects_the_capture_instance_port_whatever_the_database_name() -> anyhow::Result<()> {
    let err = match guard_against_dev_db("postgres://postgres:postgres@127.0.0.1:5440/postgres") {
        Ok(()) => anyhow::bail!("guard_against_dev_db must reject the capture instance port"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(msg.contains("mike#314"), "got: {msg}");
    assert!(msg.contains("127.0.0.1:5440"), "got: {msg}");
    Ok(())
}

#[test]
fn guard_accepts_the_test_instance_port() -> anyhow::Result<()> {
    guard_against_dev_db("postgres://postgres:postgres@127.0.0.1:6667/postgres")?;
    Ok(())
}

#[tokio::test]
async fn instance_guard_refuses_an_instance_hosting_the_marker_database() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let admin_pool = connect_single(&admin_url).await?;
    let marker = format!("claude_proxy_test_marker_{}", uuid::Uuid::new_v4().simple());
    create_empty_db(&admin_pool, &marker).await?;
    let refused = refuse_instance_hosting(&admin_url, &marker).await;
    force_drop(&admin_pool, &marker).await;
    admin_pool.close().await;
    let err = match refused {
        Ok(()) => anyhow::bail!("an instance hosting the marker database must be refused"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("mike#314"), "got: {err}");
    Ok(())
}

#[tokio::test]
async fn instance_guard_accepts_an_instance_without_the_marker_database() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    refuse_instance_hosting(&admin_url, "claude_proxy_test_no_such_marker").await?;
    guard_against_capture_instance(&admin_url).await?;
    Ok(())
}

#[tokio::test]
async fn sweep_spares_a_database_created_inside_the_minimum_age() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let admin_pool = connect_single(&admin_url).await?;
    let prefix = unique_prefix("fresh");
    let fresh = format!("{prefix}db");
    create_empty_db(&admin_pool, &fresh).await?;
    sweep_orphans(&admin_pool, &prefix, "unrelated_keep", 60).await?;
    let survived = db_exists(&admin_pool, &fresh).await?;
    force_drop(&admin_pool, &fresh).await;
    admin_pool.close().await;
    assert!(
        survived,
        "a database created seconds ago with no connection must survive a 60 minute sweep"
    );
    Ok(())
}

#[tokio::test]
async fn sweep_spares_a_database_with_a_live_connection_even_when_old_enough() -> anyhow::Result<()>
{
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let admin_pool = connect_single(&admin_url).await?;
    let prefix = unique_prefix("live");
    let in_use = format!("{prefix}db");
    create_empty_db(&admin_pool, &in_use).await?;
    let user_pool = connect_single(&replace_db_name(&admin_url, &in_use)?).await?;
    sqlx::query("SELECT 1").execute(&user_pool).await?;
    sweep_orphans(&admin_pool, &prefix, "unrelated_keep", 0).await?;
    let survived = db_exists(&admin_pool, &in_use).await?;
    user_pool.close().await;
    force_drop(&admin_pool, &in_use).await;
    admin_pool.close().await;
    assert!(survived, "a database with a live connection must survive");
    Ok(())
}

#[tokio::test]
async fn sweep_spares_the_kept_run_database_and_drops_an_old_orphan() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let admin_pool = connect_single(&admin_url).await?;
    let prefix = unique_prefix("orphan");
    let kept = format!("{prefix}kept");
    let orphan = format!("{prefix}orphan");
    create_empty_db(&admin_pool, &kept).await?;
    create_empty_db(&admin_pool, &orphan).await?;
    sweep_orphans(&admin_pool, &prefix, &kept, 0).await?;
    let kept_survived = db_exists(&admin_pool, &kept).await?;
    let orphan_survived = db_exists(&admin_pool, &orphan).await?;
    force_drop(&admin_pool, &kept).await;
    force_drop(&admin_pool, &orphan).await;
    admin_pool.close().await;
    assert!(kept_survived, "the kept run database must survive");
    assert!(
        !orphan_survived,
        "an old database with no connection must be dropped"
    );
    Ok(())
}

#[tokio::test]
async fn sweep_leaves_databases_outside_its_prefix_alone() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let admin_pool = connect_single(&admin_url).await?;
    let other = unique_prefix("other");
    let mine = unique_prefix("mine");
    let bystander = format!("{other}db");
    create_empty_db(&admin_pool, &bystander).await?;
    sweep_orphans(&admin_pool, &mine, "unrelated_keep", 0).await?;
    let survived = db_exists(&admin_pool, &bystander).await?;
    force_drop(&admin_pool, &bystander).await;
    admin_pool.close().await;
    assert!(
        survived,
        "a database outside the sweep prefix must not be touched"
    );
    Ok(())
}

const ROUNDS: usize = 25;
const CLIENTS: usize = 16;

async fn reconnecting_clients(url: String, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let mut workers = Vec::new();
    for _ in 0..CLIENTS {
        let (url, stop) = (url.clone(), stop.clone());
        workers.push(tokio::spawn(async move {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(pool) = connect_single(&url).await {
                    let _ = sqlx::query("SELECT 1").execute(&pool).await;
                    pool.close().await;
                }
            }
        }));
    }
    for worker in workers {
        let _ = worker.await;
    }
}

async fn dropped_under_reconnecting_clients(
    admin_url: &str,
    drop: impl AsyncFn(&str, &str) -> anyhow::Result<()>,
) -> anyhow::Result<usize> {
    let admin_pool = connect_single(admin_url).await?;
    let mut failures = 0;
    for _ in 0..ROUNDS {
        let db_name = format!("claude_proxy_test_drop_{}", uuid::Uuid::new_v4().simple());
        create_empty_db(&admin_pool, &db_name).await?;
        let url = replace_db_name(admin_url, &db_name)?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clients = tokio::spawn(reconnecting_clients(url, stop.clone()));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if drop(admin_url, &db_name).await.is_err() {
            failures += 1;
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = clients.await;
        force_drop(&admin_pool, &db_name).await;
    }
    admin_pool.close().await;
    Ok(failures)
}

#[tokio::test]
async fn drop_database_succeeds_while_clients_keep_reconnecting() -> anyhow::Result<()> {
    let Some(admin_url) = required_admin_url()? else {
        return Ok(());
    };
    let failures = dropped_under_reconnecting_clients(&admin_url, drop_database).await?;
    assert_eq!(
        failures, 0,
        "drop_database failed in {failures} of {ROUNDS} rounds"
    );
    Ok(())
}

#[tokio::test]
async fn acquire_returns_the_shared_run_database_and_isolated_returns_its_own() -> anyhow::Result<()>
{
    let Some(test_db) = crate::store::persistence::test_support::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let first = test_db.acquire().await?;
    let second = test_db.acquire().await?;
    assert_eq!(first.database_url, second.database_url);
    let isolated = test_db.acquire_isolated().await?;
    assert_ne!(first.database_url, isolated.database_url);
    isolated.cleanup().await?;
    Ok(())
}
