use super::*;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::change_feed::spawn_change_feed;
use crate::store::models::ProxyEvent;

const KEY: &str = "embedder-a";

struct Fed {
    feed: ChangeFeed,
    shutdown: CancellationToken,
    _events: broadcast::Sender<ProxyEvent>,
}

impl Drop for Fed {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn fed() -> Fed {
    let feed = ChangeFeed::new();
    let (events, rx) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    spawn_change_feed(&feed, None, rx, shutdown.clone(), &TaskTracker::new());
    Fed {
        feed,
        shutdown,
        _events: events,
    }
}

fn settled_empty(feed: &ChangeFeed, step: SourceStep, now: Instant) -> SourceSteps {
    let mut steps = SourceSteps::default();
    let ticket = SourceSteps::ticket(feed);
    steps.settle(step, ticket, KEY, 0, now);
    steps
}

#[tokio::test]
async fn directives_found_empty_wait_until_a_directive_is_written() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Directives, now);
    assert!(steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
    fed.feed.record(RowChangedTable::Requests, Some("s"));
    assert!(
        steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now),
        "a change to another table does not wake the directive step"
    );
    fed.feed
        .record(RowChangedTable::DossierDirectives, Some("s"));
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
}

#[tokio::test]
async fn directives_found_empty_run_again_when_the_embedder_changes() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Directives, now);
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, "embedder-b", now));
}

#[tokio::test]
async fn directives_found_empty_run_again_after_a_listener_resync() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Directives, now);
    fed.feed.resync();
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
}

#[tokio::test]
async fn directives_found_empty_run_again_after_the_backstop() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Directives, now);
    let later = now + SOURCE_BACKSTOP;
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, later));
}

#[tokio::test]
async fn directive_written_during_the_pass_runs_the_next_tick() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = SourceSteps::default();
    let ticket = SourceSteps::ticket(&fed.feed);
    fed.feed.record(RowChangedTable::DossierDirectives, None);
    steps.settle(SourceStep::Directives, ticket, KEY, 0, now);
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
}

#[tokio::test]
async fn directives_that_found_rows_run_again_the_next_tick() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = SourceSteps::default();
    let ticket = SourceSteps::ticket(&fed.feed);
    steps.settle(SourceStep::Directives, ticket, KEY, 3, now);
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
}

#[test]
fn directives_run_every_tick_without_a_change_feed() {
    let feed = ChangeFeed::new();
    let now = Instant::now();
    let mut steps = settled_empty(&feed, SourceStep::Directives, now);
    assert!(!steps.nothing_new(SourceStep::Directives, &feed, KEY, now));
}

#[tokio::test]
async fn anchors_found_empty_wait_until_an_anchor_is_written() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Anchors, now);
    assert!(steps.nothing_new(SourceStep::Anchors, &fed.feed, KEY, now));
    fed.feed.record(RowChangedTable::DossierDirectives, None);
    assert!(
        steps.nothing_new(SourceStep::Anchors, &fed.feed, KEY, now),
        "a new directive does not wake the anchor step"
    );
    fed.feed.record(RowChangedTable::SemanticAnchors, None);
    assert!(!steps.nothing_new(SourceStep::Anchors, &fed.feed, KEY, now));
}

#[tokio::test]
async fn anchors_found_empty_run_again_when_the_embedder_changes() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Anchors, now);
    assert!(steps.nothing_new(SourceStep::Anchors, &fed.feed, KEY, now));
    assert!(!steps.nothing_new(SourceStep::Anchors, &fed.feed, "embedder-b", now));
}

#[tokio::test]
async fn anchor_and_directive_steps_keep_separate_marks() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Anchors, now);
    assert!(!steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
    assert!(steps.nothing_new(SourceStep::Anchors, &fed.feed, KEY, now));
}

#[tokio::test]
async fn corrections_found_empty_wait_until_a_turn_is_qualified_as_corrected() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Corrections, now);
    assert!(steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, now));
    fed.feed.record(RowChangedTable::Requests, Some("s"));
    fed.feed.record(RowChangedTable::SemanticAnchors, None);
    assert!(
        steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, now),
        "a request or an anchor does not wake the tagger"
    );
    fed.feed
        .record(RowChangedTable::CorrectionQualifications, None);
    assert!(!steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, now));
}

#[tokio::test]
async fn corrections_found_empty_run_again_after_the_worker_builds_turn_items() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Corrections, now);
    assert!(steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, now));
    steps.forget(SourceStep::Corrections);
    assert!(!steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, now));
}

#[tokio::test]
async fn corrections_found_empty_run_again_after_the_backstop_or_an_embedder_change() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Corrections, now);
    assert!(!steps.nothing_new(SourceStep::Corrections, &fed.feed, "embedder-b", now));
    let later = now + SOURCE_BACKSTOP;
    assert!(!steps.nothing_new(SourceStep::Corrections, &fed.feed, KEY, later));
}

#[tokio::test]
async fn forgetting_corrections_keeps_the_other_marks() {
    let fed = fed();
    let now = Instant::now();
    let mut steps = settled_empty(&fed.feed, SourceStep::Directives, now);
    steps.settle(
        SourceStep::Corrections,
        SourceSteps::ticket(&fed.feed),
        KEY,
        0,
        now,
    );
    steps.forget(SourceStep::Corrections);
    assert!(steps.nothing_new(SourceStep::Directives, &fed.feed, KEY, now));
}
