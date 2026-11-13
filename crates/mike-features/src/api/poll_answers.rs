use std::collections::HashMap;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Serialize;

use crate::api::workspace::RosterAnswer;
use crate::change_feed::{ChangeFeed, Generation};
use crate::launch::state::LaunchState;
use crate::store::models::RowChangedTable;

#[cfg(test)]
#[path = "poll_answers_tests.rs"]
pub mod poll_answers_tests;

#[cfg(test)]
#[path = "poll_answers_db_tests.rs"]
pub mod poll_answers_db_tests;

const MAX_ENTRIES: usize = 2048;
const MAX_WEIGHT: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Freshness {
    pub tables: &'static [RowChangedTable],
    pub per_session: &'static [RowChangedTable],
    pub ttl: Duration,
}

pub const SESSION_MAILBOX: Freshness = Freshness {
    tables: &[],
    per_session: &[RowChangedTable::MailboxMessages],
    ttl: Duration::from_secs(30),
};

pub const MAILBOX_THREADS: Freshness = Freshness {
    tables: &[RowChangedTable::MailboxThreads],
    per_session: &[RowChangedTable::MailboxMessages, RowChangedTable::Sessions],
    ttl: Duration::from_secs(30),
};

pub const ROSTER: Freshness = Freshness {
    tables: &[],
    per_session: &[RowChangedTable::Sessions],
    ttl: Duration::from_secs(15),
};

pub const FLEET: Freshness = Freshness {
    tables: &[
        RowChangedTable::LaunchLaunches,
        RowChangedTable::LaunchFleets,
    ],
    per_session: &[],
    ttl: Duration::from_secs(10),
};

pub struct Poll<'a> {
    pub key: String,
    pub freshness: Freshness,
    pub scope: Vec<String>,
    pub headers: &'a HeaderMap,
}

pub struct Loaded<V> {
    pub value: V,
    pub sessions: Vec<String>,
}

pub struct Presented {
    pub body: Bytes,
    pub live: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct PollError(pub StatusCode);

impl From<PollError> for Response {
    fn from(error: PollError) -> Self {
        error.0.into_response()
    }
}

pub trait Weigh {
    fn weight(&self) -> usize;
}

impl Weigh for Bytes {
    fn weight(&self) -> usize {
        self.len()
    }
}

struct Entry<V> {
    etag: String,
    sessions: Vec<String>,
    value: V,
    weight: usize,
}

struct Shelf<V> {
    entries: HashMap<String, Entry<V>>,
    weight: usize,
}

pub struct AnswerCache<V> {
    shelf: Mutex<Shelf<V>>,
}

impl<V> Default for AnswerCache<V> {
    fn default() -> Self {
        Self {
            shelf: Mutex::new(Shelf {
                entries: HashMap::new(),
                weight: 0,
            }),
        }
    }
}

impl<V: Clone + Weigh> AnswerCache<V> {
    fn shelf(&self) -> MutexGuard<'_, Shelf<V>> {
        self.shelf.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sessions(&self, key: &str) -> Option<Vec<String>> {
        self.shelf()
            .entries
            .get(key)
            .map(|entry| entry.sessions.clone())
    }

    fn value_at(&self, key: &str, etag: &str) -> Option<V> {
        self.shelf()
            .entries
            .get(key)
            .filter(|entry| entry.etag == etag)
            .map(|entry| entry.value.clone())
    }

    fn store(&self, key: String, etag: String, sessions: Vec<String>, value: V) {
        let weight = value.weight();
        let mut shelf = self.shelf();
        if let Some(old) = shelf.entries.remove(&key) {
            shelf.weight = shelf.weight.saturating_sub(old.weight);
        }
        if weight > MAX_WEIGHT {
            return;
        }
        if shelf.entries.len() >= MAX_ENTRIES || shelf.weight + weight > MAX_WEIGHT {
            shelf.entries.clear();
            shelf.weight = 0;
        }
        shelf.weight += weight;
        shelf.entries.insert(
            key,
            Entry {
                etag,
                sessions,
                value,
                weight,
            },
        );
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.shelf().entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Default)]
struct Shelves {
    bodies: AnswerCache<Bytes>,
    rosters: AnswerCache<RosterAnswer>,
    frozen_at: AtomicU64,
}

#[derive(Clone, Default)]
pub struct PollAnswers {
    shelves: Arc<Shelves>,
}

impl PollAnswers {
    pub fn bodies(&self) -> &AnswerCache<Bytes> {
        &self.shelves.bodies
    }

    pub fn rosters(&self) -> &AnswerCache<RosterAnswer> {
        &self.shelves.rosters
    }

    fn now(&self) -> SystemTime {
        match self.shelves.frozen_at.load(Ordering::Relaxed) {
            0 => SystemTime::now(),
            secs => UNIX_EPOCH + Duration::from_secs(secs),
        }
    }

    #[cfg(test)]
    pub fn freeze_clock(&self, unix_secs: u64) {
        self.shelves.frozen_at.store(unix_secs, Ordering::Relaxed);
    }
}

struct Stamp {
    etag: String,
    newest: Generation,
}

fn key_hash(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn bucket(key: &str, ttl: Duration, now: SystemTime) -> u64 {
    let width = ttl.as_secs().max(1);
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    (secs + key_hash(key) % width) / width
}

fn stamp(
    feed: &ChangeFeed,
    key: &str,
    freshness: Freshness,
    sessions: &[String],
    now: SystemTime,
) -> Stamp {
    let generations = feed.generations(freshness.tables, freshness.per_session, sessions);
    let newest = generations.iter().copied().max().unwrap_or(0);
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    sessions.hash(&mut hasher);
    generations.hash(&mut hasher);
    bucket(key, freshness.ttl, now).hash(&mut hasher);
    Stamp {
        etag: format!("\"{:x}-{:016x}\"", feed.epoch(), hasher.finish()),
        newest,
    }
}

fn with_live(etag: &str, live: Option<u64>) -> String {
    match live {
        None => etag.to_owned(),
        Some(live) => format!("{}-{live:016x}\"", etag.trim_end_matches('"')),
    }
}

fn sorted_unique(mut sessions: Vec<String>) -> Vec<String> {
    sessions.sort_unstable();
    sessions.dedup();
    sessions
}

fn mark(response: &mut Response, etag: Option<&str>) {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if let Some(value) = etag.and_then(|etag| HeaderValue::from_str(etag).ok()) {
        headers.insert(header::ETAG, value);
    }
}

fn client_has(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|wanted| crate::web_base::http::if_none_match_contains(wanted, etag))
}

fn respond<V>(
    value: &V,
    base: Option<&str>,
    headers: &HeaderMap,
    present: &impl Fn(&V) -> Result<Presented, PollError>,
) -> Response {
    let presented = match present(value) {
        Ok(presented) => presented,
        Err(error) => return error.into(),
    };
    let etag = base.map(|base| with_live(base, presented.live));
    if let Some(etag) = etag.as_deref().filter(|etag| client_has(headers, etag)) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        mark(&mut response, Some(etag));
        return response;
    }
    let mut response = (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        presented.body,
    )
        .into_response();
    mark(&mut response, etag.as_deref());
    response
}

pub async fn answer<V, Fut>(
    state: &LaunchState,
    cache: &AnswerCache<V>,
    poll: Poll<'_>,
    load: impl FnOnce() -> Fut,
    present: impl Fn(&V) -> Result<Presented, PollError>,
) -> Response
where
    V: Clone + Weigh,
    Fut: Future<Output = Result<Loaded<V>, Response>>,
{
    let feed = &state.change_feed;
    if !feed.is_fed() {
        return match load().await {
            Ok(loaded) => respond(&loaded.value, None, poll.headers, &present),
            Err(response) => response,
        };
    }
    let now = state.poll_answers.now();
    let remembered = cache
        .sessions(&poll.key)
        .unwrap_or_else(|| sorted_unique(poll.scope.clone()));
    let current = stamp(feed, &poll.key, poll.freshness, &remembered, now);
    if let Some(value) = cache.value_at(&poll.key, &current.etag) {
        return respond(&value, Some(&current.etag), poll.headers, &present);
    }
    let watermark = feed.generation();
    let loaded = match load().await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    let mut sessions = poll.scope;
    sessions.extend(loaded.sessions);
    let sessions = sorted_unique(sessions);
    let fresh = stamp(feed, &poll.key, poll.freshness, &sessions, now);
    if fresh.newest > watermark {
        return respond(&loaded.value, None, poll.headers, &present);
    }
    let response = respond(&loaded.value, Some(&fresh.etag), poll.headers, &present);
    cache.store(poll.key, fresh.etag, sessions, loaded.value);
    response
}

pub async fn answer_json<Fut>(
    state: &LaunchState,
    poll: Poll<'_>,
    load: impl FnOnce() -> Fut,
) -> Response
where
    Fut: Future<Output = Result<Loaded<Bytes>, Response>>,
{
    answer(state, state.poll_answers.bodies(), poll, load, |body| {
        Ok(Presented {
            body: body.clone(),
            live: None,
        })
    })
    .await
}

pub fn json_bytes<T: Serialize>(value: &T) -> Result<Bytes, PollError> {
    serde_json::to_vec(value).map(Bytes::from).map_err(|error| {
        tracing::warn!(%error, "poll answer could not be serialised");
        PollError(StatusCode::INTERNAL_SERVER_ERROR)
    })
}

pub fn live_hash(flags: impl Iterator<Item = bool>) -> u64 {
    let mut hasher = DefaultHasher::new();
    for flag in flags {
        flag.hash(&mut hasher);
    }
    hasher.finish()
}
