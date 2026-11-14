use std::time::Duration;

use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{Queued, begin_check};
use crate::launch::fleet::check::Runner;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::managed_queue::{self, Enlisted, Lane};
use crate::launch::fleet::managed_test_support::{running_fleet, with_private_root};
use crate::launch::fleet::operation::declared_timeout;
use crate::launch::fleet::owned_execution::{Phase, Registration, registry};
use crate::launch::fleet::owned_spawn_fence::FENCE_TIMEOUT_OVERRIDE;
use crate::launch::fleet::test_fixtures::{complete, parsed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn definition(command: &str, repo: &std::path::Path) -> FleetDefinition {
    let mut def = parsed(&complete());
    def.repo = repo.to_string_lossy().into_owned();
    def.check.command = command.to_string();
    def
}

async fn ahead_of_us(
    run: &str,
    def: &FleetDefinition,
    count: i64,
) -> anyhow::Result<Vec<Registration>> {
    let fence = registry().fence(run).await;
    let mut registrations = Vec::new();
    for n in 0..count {
        let ledger = format!("{run}-ahead-{n}");
        let registration = fence.register(&ledger).map_err(anyhow::Error::msg)?;
        managed_queue::enlist(
            8_000_000 + n,
            &Enlisted {
                repo: &def.repo,
                lane: Lane::Check,
                ledger: &ledger,
                seat: "other",
                state: "c0",
            },
            registration.owner(),
        );
        registrations.push(registration);
    }
    Ok(registrations)
}

async fn ask(
    persist: &PersistHandle,
    def: &FleetDefinition,
    (run, fleet): (&str, Uuid),
) -> anyhow::Result<Result<Queued, String>> {
    let tracker = TaskTracker::new();
    let runner = Runner {
        persist,
        tracker: &tracker,
        run,
        fleet,
    };
    with_private_root(begin_check(&runner, def, "state-1", "seat-x")).await
}

#[tokio::test]
async fn a_check_without_a_declared_timeout_queues_behind_a_long_line_and_reports_its_place()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let repo = tempfile::tempdir()?;
    let command = format!("long-line-{}", Uuid::new_v4());
    let def = definition(&command, repo.path());
    assert_eq!(declared_timeout(&def), None);
    let run = format!("receipt-line-{}", Uuid::new_v4());
    let fleet = running_fleet(&persist, &def, &run).await?;
    managed_queue::remember_measurement(&command, Duration::from_secs(1800));
    let ahead = ahead_of_us(&run, &def, 4).await?;

    let queued = ask(&persist, &def, (&run, fleet))
        .await?
        .map_err(anyhow::Error::msg)?;
    let place = managed_queue::place(queued.id, managed_queue::measured(&command))
        .ok_or_else(|| anyhow::anyhow!("the receipt is not in the queue"))?;
    assert_eq!(place.ahead_waiting, 4);
    assert!(
        place.describe().contains("place 5 in line"),
        "{}",
        place.describe()
    );
    assert!(persist.fleet().tree_check(queued.id).await?.is_some());

    managed_queue::leave(Some(queued.id));
    queued.registration.receipt_recorded();
    for (n, registration) in ahead.into_iter().enumerate() {
        managed_queue::leave(Some(8_000_000 + i64::try_from(n)?));
        registration.receipt_recorded();
    }
    db.cleanup().await?;
    Ok(())
}

async fn slow_trigger(persist: &PersistHandle, command: &str, body: &str) -> anyhow::Result<()> {
    let sql = format!(
        "CREATE FUNCTION slow_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} RETURN NULL; END $$;
         CREATE CONSTRAINT TRIGGER slow_commit AFTER INSERT ON launch.tree_checks
           DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command = '{command}')
           EXECUTE FUNCTION slow_commit();"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(persist.pool())
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_commit_that_outlasts_the_fence_timeout_is_read_back_from_the_database_not_blocked()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let repo = tempfile::tempdir()?;
    let command = "slow commit";
    slow_trigger(&persist, command, "PERFORM pg_sleep(2);").await?;
    let def = definition(command, repo.path());
    let run = format!("receipt-slow-{}", Uuid::new_v4());
    let fleet = running_fleet(&persist, &def, &run).await?;

    let queued = FENCE_TIMEOUT_OVERRIDE
        .scope(
            Duration::from_millis(500),
            ask(&persist, &def, (&run, fleet)),
        )
        .await?
        .map_err(anyhow::Error::msg)?;
    assert!(persist.fleet().tree_check(queued.id).await?.is_some());
    let blocked = registry()
        .snapshot(&run)
        .pending
        .iter()
        .any(|entry| entry.phase == Phase::ReceiptBlocked);
    assert!(!blocked, "a slow commit must not leave the receipt blocked");

    managed_queue::leave(Some(queued.id));
    queued.registration.receipt_recorded();
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_commit_that_fails_is_read_back_as_not_created_and_leaves_nothing_blocked()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let repo = tempfile::tempdir()?;
    let command = "refused commit";
    slow_trigger(&persist, command, "RAISE EXCEPTION 'commit refused';").await?;
    let def = definition(command, repo.path());
    let run = format!("receipt-refused-{}", Uuid::new_v4());
    let fleet = running_fleet(&persist, &def, &run).await?;

    let refused = ask(&persist, &def, (&run, fleet)).await?;
    let why = refused.err().ok_or_else(|| anyhow::anyhow!("queued"))?;
    assert!(why.contains("no receipt was created"), "{why}");
    assert!(registry().snapshot(&run).pending.is_empty());
    assert!(persist.fleet().latest_tree_check(&run).await?.is_none());
    db.cleanup().await?;
    Ok(())
}
