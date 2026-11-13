use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use super::{
    AnswerCache, Loaded, MAX_ENTRIES, MAX_WEIGHT, Poll, PollError, Presented, SESSION_MAILBOX,
    answer, answer_json,
};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::models::RowChangedTable;

const OWN: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const OTHER: &str = "bbbbbbbb-0000-4000-8000-000000000002";
const PEER: &str = "cccccccc-0000-4000-8000-000000000003";
const FROZEN_AT: u64 = 1_800_000_000;

async fn state() -> anyhow::Result<AppState> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.poll_answers.freeze_clock(FROZEN_AT);
    state.spawn_change_feed();
    Ok(state)
}

fn conditional(etag: Option<&str>) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(etag) = etag {
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(etag)?);
    }
    Ok(headers)
}

struct Probe {
    loads: AtomicUsize,
    body: std::sync::Mutex<String>,
    sessions: Vec<String>,
}

impl Probe {
    fn new(sessions: &[&str]) -> Self {
        Self {
            loads: AtomicUsize::new(0),
            body: std::sync::Mutex::new("{\"n\":0}".to_owned()),
            sessions: sessions.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn set_body(&self, body: &str) {
        if let Ok(mut held) = self.body.lock() {
            body.clone_into(&mut *held);
        }
    }

    fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }

    async fn load(&self) -> Result<Loaded<Bytes>, Response> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        let body = self
            .body
            .lock()
            .map(|held| held.clone())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
        Ok(Loaded {
            value: Bytes::from(body),
            sessions: self.sessions.clone(),
        })
    }
}

async fn ask(state: &AppState, probe: &Probe, etag: Option<&str>) -> anyhow::Result<Response> {
    let headers = conditional(etag)?;
    let poll = Poll {
        key: format!("test-messages:{OWN}"),
        freshness: SESSION_MAILBOX,
        scope: vec![OWN.to_owned()],
        headers: &headers,
    };
    Ok(answer_json(&state.launch_state(), poll, || probe.load()).await)
}

fn etag_of(response: &Response) -> anyhow::Result<String> {
    let value = response
        .headers()
        .get(header::ETAG)
        .ok_or_else(|| anyhow::anyhow!("no ETag on a {} answer", response.status()))?;
    Ok(value.to_str()?.to_owned())
}

async fn body_of(response: Response) -> anyhow::Result<String> {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok(String::from_utf8(bytes.to_vec())?)
}

async fn primed(state: &AppState, probe: &Probe) -> anyhow::Result<String> {
    let first = ask(state, probe, None).await?;
    assert_eq!(first.status(), StatusCode::OK);
    etag_of(&first)
}

#[tokio::test]
async fn poll_with_no_running_feed_loads_every_answer_and_offers_no_etag() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let probe = Probe::new(&[]);

    let first = ask(&state, &probe, None).await?;
    assert_eq!(first.status(), StatusCode::OK);
    assert!(first.headers().get(header::ETAG).is_none());

    probe.set_body("{\"n\":1}");
    let second = ask(&state, &probe, None).await?;
    assert_eq!(body_of(second).await?, "{\"n\":1}");
    assert_eq!(
        probe.loads(),
        2,
        "nothing would ever invalidate a stored answer"
    );
    Ok(())
}

#[tokio::test]
async fn poll_unchanged_generation_answers_304_without_loading() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;
    assert_eq!(probe.loads(), 1);

    let again = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(etag_of(&again)?, etag);
    assert_eq!(probe.loads(), 1, "a 304 must not read the database");
    Ok(())
}

#[tokio::test]
async fn poll_second_client_without_etag_is_served_from_memory() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;

    let other_client = ask(&state, &probe, None).await?;
    assert_eq!(other_client.status(), StatusCode::OK);
    assert_eq!(etag_of(&other_client)?, etag);
    assert_eq!(body_of(other_client).await?, "{\"n\":0}");
    assert_eq!(probe.loads(), 1);
    Ok(())
}

#[tokio::test]
async fn poll_write_for_another_session_still_answers_304() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;

    state
        .change_feed
        .record(RowChangedTable::MailboxMessages, Some(OTHER));
    state
        .change_feed
        .record(RowChangedTable::Requests, Some(OWN));
    let again = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(probe.loads(), 1);
    Ok(())
}

#[tokio::test]
async fn poll_write_for_own_session_answers_200_with_the_new_body() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;

    probe.set_body("{\"n\":1}");
    state
        .change_feed
        .record(RowChangedTable::MailboxMessages, Some(OWN));
    let changed = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_ne!(etag_of(&changed)?, etag);
    assert_eq!(body_of(changed).await?, "{\"n\":1}");
    assert_eq!(probe.loads(), 2);
    Ok(())
}

#[tokio::test]
async fn poll_write_for_a_remembered_dependency_answers_200() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[PEER]);
    let etag = primed(&state, &probe).await?;

    state
        .change_feed
        .record(RowChangedTable::MailboxMessages, Some(PEER));
    let changed = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(probe.loads(), 2);
    Ok(())
}

#[tokio::test]
async fn poll_resync_answers_200() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;

    state.change_feed.resync();
    let after = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(probe.loads(), 2);
    Ok(())
}

#[tokio::test]
async fn poll_ttl_window_rollover_answers_200() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&state, &probe).await?;

    state
        .poll_answers
        .freeze_clock(FROZEN_AT + SESSION_MAILBOX.ttl.as_secs());
    let after = ask(&state, &probe, Some(&etag)).await?;
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(probe.loads(), 2);
    Ok(())
}

#[tokio::test]
async fn poll_etag_from_another_process_answers_200() -> anyhow::Result<()> {
    let first = state().await?;
    let restarted = state().await?;
    let probe = Probe::new(&[]);
    let etag = primed(&first, &probe).await?;

    let after = ask(&restarted, &probe, Some(&etag)).await?;
    assert_eq!(after.status(), StatusCode::OK);
    assert_ne!(etag_of(&after)?, etag);
    Ok(())
}

#[tokio::test]
async fn poll_change_during_load_answers_without_etag_and_is_not_kept() -> anyhow::Result<()> {
    let state = state().await?;
    let probe = Probe::new(&[]);
    let headers = conditional(None)?;
    let poll = Poll {
        key: format!("test-messages:{OWN}"),
        freshness: SESSION_MAILBOX,
        scope: vec![OWN.to_owned()],
        headers: &headers,
    };
    let (feed, prober) = (&state.change_feed, &probe);
    let racing = answer_json(&state.launch_state(), poll, || async move {
        feed.record(RowChangedTable::MailboxMessages, Some(OWN));
        prober.load().await
    })
    .await;
    assert_eq!(racing.status(), StatusCode::OK);
    assert!(racing.headers().get(header::ETAG).is_none());
    assert_eq!(state.poll_answers.bodies().len(), 0);
    Ok(())
}

#[tokio::test]
async fn poll_failed_load_is_passed_through_and_not_kept() -> anyhow::Result<()> {
    let state = state().await?;
    let headers = conditional(None)?;
    let poll = Poll {
        key: "test-failing".to_owned(),
        freshness: SESSION_MAILBOX,
        scope: Vec::new(),
        headers: &headers,
    };
    let failed = answer_json(&state.launch_state(), poll, || async {
        Err::<Loaded<Bytes>, Response>(StatusCode::NOT_FOUND.into_response())
    })
    .await;
    assert_eq!(failed.status(), StatusCode::NOT_FOUND);
    assert_eq!(state.poll_answers.bodies().len(), 0);
    Ok(())
}

fn live_poll(headers: &HeaderMap) -> Poll<'_> {
    Poll {
        key: "test-live".to_owned(),
        freshness: SESSION_MAILBOX,
        scope: Vec::new(),
        headers,
    }
}

#[tokio::test]
async fn poll_live_part_change_answers_200_from_memory() -> anyhow::Result<()> {
    let state = state().await?;
    let cache: AnswerCache<Bytes> = AnswerCache::default();
    let probe = Probe::new(&[]);
    let busy = AtomicBool::new(false);
    let present = |body: &Bytes| -> Result<Presented, PollError> {
        Ok(Presented {
            body: body.clone(),
            live: Some(u64::from(busy.load(Ordering::SeqCst))),
        })
    };
    let headers = conditional(None)?;
    let first = answer(
        &state.launch_state(),
        &cache,
        live_poll(&headers),
        || probe.load(),
        present,
    )
    .await;
    let etag = etag_of(&first)?;

    let same = conditional(Some(&etag))?;
    let unchanged = answer(
        &state.launch_state(),
        &cache,
        live_poll(&same),
        || probe.load(),
        present,
    )
    .await;
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);

    busy.store(true, Ordering::SeqCst);
    let changed = answer(
        &state.launch_state(),
        &cache,
        live_poll(&same),
        || probe.load(),
        present,
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_ne!(etag_of(&changed)?, etag);
    assert_eq!(
        probe.loads(),
        1,
        "a live-only change is answered from memory"
    );
    Ok(())
}

#[test]
fn answer_cache_entries_over_the_limit_empty_the_shelf() {
    let cache: AnswerCache<Bytes> = AnswerCache::default();
    for n in 0..MAX_ENTRIES {
        cache.store(
            format!("k{n}"),
            "\"e\"".to_owned(),
            Vec::new(),
            Bytes::new(),
        );
    }
    assert_eq!(cache.len(), MAX_ENTRIES);
    cache.store(
        "one more".to_owned(),
        "\"e\"".to_owned(),
        Vec::new(),
        Bytes::new(),
    );
    assert_eq!(cache.len(), 1);
    assert!(cache.value_at("one more", "\"e\"").is_some());
}

#[test]
fn answer_cache_value_heavier_than_the_shelf_is_not_kept() {
    let cache: AnswerCache<Bytes> = AnswerCache::default();
    let heavy = Bytes::from(vec![0_u8; MAX_WEIGHT + 1]);
    cache.store("heavy".to_owned(), "\"e\"".to_owned(), Vec::new(), heavy);
    assert_eq!(cache.len(), 0);
}
