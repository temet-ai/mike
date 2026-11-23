//! Fleet loops tick only while a fleet is live. A loop whose tick found
//! nothing to do waits for a change to `launch.fleets` or `launch.launches`
//! instead of polling; without a running change feed it keeps its fixed
//! interval.

use std::time::Duration;

use crate::base_state::BaseState;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::proxy_event::RowChangedTable;

/// How long an idle loop sleeps when nothing changes, in case a change
/// notice was missed.
pub const BACKSTOP: Duration = Duration::from_secs(600);
pub const WATCHED: [RowChangedTable; 2] =
    [RowChangedTable::LaunchFleets, RowChangedTable::LaunchLaunches];

/// Waits before a fleet loop's next tick. `since` is the feed generation read
/// before the tick, so a change made during the tick wakes the loop at once.
/// Returns false once the proxy is shutting down.
pub async fn pause(state: &BaseState, busy: bool, interval: Duration, since: Generation) -> bool {
    let shutdown = state.shutdown();
    let wait = wait_for_work(&state.change_feed, busy, interval, since);
    tokio::select! {
        biased;
        () = shutdown.cancelled() => false,
        () = wait => !shutdown.is_cancelled(),
    }
}

async fn wait_for_work(feed: &ChangeFeed, busy: bool, interval: Duration, since: Generation) {
    if busy || !feed.is_fed() {
        tokio::time::sleep(interval).await;
    } else {
        feed.wait_for_change(&WATCHED, since, BACKSTOP).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed_feed() -> ChangeFeed {
        let feed = ChangeFeed::default();
        let (_tx, rx) = tokio::sync::broadcast::channel(4);
        crate::change_feed::spawn_change_feed(
            &feed,
            None,
            rx,
            tokio_util::sync::CancellationToken::new(),
            &tokio_util::task::TaskTracker::new(),
        );
        feed
    }

    const TICK: Duration = Duration::from_millis(20);

    const DISPLAY: &str = include_str!("display.rs");
    const BRIEF_REFRESH: &str = include_str!("brief_version.rs");
    const RETROSPECTIVE: &str = include_str!("retrospective_worker.rs");

    #[test]
    fn the_display_tick_reads_seat_rows_only_through_assemble_with_rows() {
        assert!(!DISPLAY.contains("fleet_seat_rows"));
        assert!(DISPLAY.contains("assemble_with_rows(state, row)"));
        assert_eq!(DISPLAY.matches("assemble_with_rows(").count(), 1);
    }

    #[test]
    fn every_fleet_loop_waits_through_the_idle_pause() {
        for (name, source) in [
            ("display", DISPLAY),
            ("brief refresh", BRIEF_REFRESH),
            ("retrospective", RETROSPECTIVE),
        ] {
            assert!(
                source.contains("super::idle::pause("),
                "the {name} loop must wait through idle::pause"
            );
            assert!(
                !source.contains("tokio::time::sleep(") && !source.contains("interval("),
                "the {name} loop must not keep its own fixed timer"
            );
        }
    }

    async fn finishes(feed: &ChangeFeed, busy: bool, since: Generation) -> bool {
        tokio::time::timeout(
            Duration::from_millis(300),
            wait_for_work(feed, busy, TICK, since),
        )
        .await
        .is_ok()
    }

    #[tokio::test]
    async fn an_idle_loop_does_not_tick_while_nothing_changes() {
        let feed = fed_feed();
        assert!(!finishes(&feed, false, feed.generation()).await);
    }

    #[tokio::test]
    async fn a_busy_loop_ticks_on_its_interval() {
        let feed = fed_feed();
        assert!(finishes(&feed, true, feed.generation()).await);
    }

    #[tokio::test]
    async fn a_fleet_change_wakes_an_idle_loop() {
        let feed = fed_feed();
        let since = feed.generation();
        feed.record(RowChangedTable::LaunchFleets, None);
        assert!(finishes(&feed, false, since).await);
    }

    #[tokio::test]
    async fn a_launch_change_wakes_an_idle_loop() {
        let feed = fed_feed();
        let since = feed.generation();
        feed.record(RowChangedTable::LaunchLaunches, None);
        assert!(finishes(&feed, false, since).await);
    }

    #[tokio::test]
    async fn an_unrelated_change_leaves_an_idle_loop_waiting() {
        let feed = fed_feed();
        let since = feed.generation();
        feed.record(RowChangedTable::Requests, None);
        assert!(!finishes(&feed, false, since).await);
    }

    #[tokio::test]
    async fn without_a_change_feed_the_fixed_interval_stays() {
        let feed = ChangeFeed::default();
        assert!(finishes(&feed, false, feed.generation()).await);
    }
}
