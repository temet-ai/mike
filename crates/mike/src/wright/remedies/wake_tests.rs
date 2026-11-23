use std::time::Duration;

use tokio::time::Instant;

use super::{Action, MAX_GAP, Wake};

#[test]
fn the_first_tick_runs_a_pass() {
    let mut wake = Wake::default();
    assert!(wake.due(None, Instant::now()));
}

#[test]
fn a_new_watch_run_wakes_the_runner_and_the_same_run_does_not() {
    let mut wake = Wake::default();
    let start = Instant::now();
    assert!(wake.due(Some(4), start));
    assert!(!wake.due(Some(4), start + Duration::from_secs(15)));
    assert!(wake.due(Some(5), start + Duration::from_secs(30)));
}

#[test]
fn an_idle_runner_passes_again_after_the_longest_gap() {
    let mut wake = Wake::default();
    let start = Instant::now();
    assert!(wake.due(Some(4), start));
    assert!(wake.due(Some(4), start + MAX_GAP));
}

#[test]
fn every_action_code_round_trips_and_only_reversible_ones_have_an_undo() {
    for action in Action::ALL {
        assert_eq!(Action::from_code(action.code()), Some(action));
    }
    let undoable: Vec<&str> = Action::ALL
        .into_iter()
        .filter(|a| a.can_undo())
        .map(Action::code)
        .collect();
    assert_eq!(undoable, ["set_setting", "disable_filter_rule"]);
    assert_eq!(Action::from_code("restart_proxy"), None);
}

fn fed_owners() -> anyhow::Result<(super::Owners, crate::change_feed::ChangeFeed)> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://127.0.0.1:1/remedies_wake_fixture")?;
    let persist = crate::store::persistence::PersistHandle::from_pools(
        crate::store::persistence::pool_classes::PoolClasses::single(pool),
    );
    let feed = crate::change_feed::ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        Some(&persist),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    let owners = super::Owners {
        persist,
        settings: None,
        filters: None,
    };
    Ok((owners, feed))
}

#[tokio::test]
async fn with_a_fed_feed_a_new_check_run_is_learned_from_the_feed_without_a_read()
-> anyhow::Result<()> {
    let (owners, feed) = fed_owners()?;
    let first = super::latest_check_run(&owners).await?;
    assert_eq!(super::latest_check_run(&owners).await?, first);
    feed.record(crate::store::models::RowChangedTable::Requests, None);
    assert_eq!(super::latest_check_run(&owners).await?, first);
    feed.record(crate::store::models::RowChangedTable::WrightCheckRuns, None);
    assert_ne!(super::latest_check_run(&owners).await?, first);
    Ok(())
}
