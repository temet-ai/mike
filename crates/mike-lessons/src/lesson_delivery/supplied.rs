use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::time::Duration;

use uuid::Uuid;

use super::held::find;
use super::origin::{heading_block_ids, tether_line_ids};
use crate::memory_keeper::block::KEEPER_MARKER_OPEN;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::supplied_items::{CarriedCard, SuppliedLesson};

const CARD_HASH_CHARS: usize = 12;
const CARD_RECORD_RETRY: Duration = Duration::from_secs(2);

pub use crate::lesson_cache::scores::{chosen_score, remember_scores};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SupplyEvidence {
    pub lessons: Vec<SuppliedLesson>,
    pub cards: Vec<CarriedCard>,
}

pub fn read_evidence(
    session: &str,
    original: &[u8],
    sent: &[u8],
    proxy_lessons: &[i64],
) -> SupplyEvidence {
    let mut lessons = Vec::new();
    let routes = [
        ("proxy", proxy_lessons.to_vec()),
        ("mod", heading_block_ids(original)),
        ("tether", tether_line_ids(original)),
    ];
    for (route, ids) in routes {
        for lesson_id in ids {
            if lessons
                .iter()
                .any(|l: &SuppliedLesson| l.lesson_id == lesson_id)
            {
                continue;
            }
            lessons.push(SuppliedLesson {
                lesson_id,
                route,
                score: chosen_score(session, lesson_id),
            });
        }
    }
    let carried = card_hashes(original);
    let mut cards: Vec<CarriedCard> = carried
        .iter()
        .map(|hash| CarriedCard {
            hash: hash.clone(),
            added_by_proxy: false,
        })
        .collect();
    cards.extend(
        card_hashes(sent)
            .into_iter()
            .filter(|hash| !carried.contains(hash))
            .map(|hash| CarriedCard {
                hash,
                added_by_proxy: true,
            }),
    );
    SupplyEvidence { lessons, cards }
}

pub fn card_hashes(body: &[u8]) -> Vec<String> {
    let marker = KEEPER_MARKER_OPEN.as_bytes();
    let mut hashes = Vec::new();
    let mut from = 0;
    while let Some(offset) = find(body.get(from..).unwrap_or_default(), marker) {
        let start = from + offset + marker.len();
        from = start;
        let Some(hash) = card_hash_at(body, start) else {
            continue;
        };
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    }
    hashes
}

fn card_hash_at(body: &[u8], start: usize) -> Option<String> {
    let end = start + CARD_HASH_CHARS;
    let hash = body.get(start..end)?;
    if body.get(end) != Some(&b']') || !hash.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    std::str::from_utf8(hash).ok().map(str::to_owned)
}

pub use crate::state::lesson_hooks::SupplyCapture;

pub fn spawn_supply_records(state: &AppState, capture: SupplyCapture<'_>) {
    let (Some(persist), Ok(session)) = (&state.persist, Uuid::parse_str(capture.session_id)) else {
        return;
    };
    let evidence = read_evidence(
        capture.session_id,
        capture.original,
        capture.sent,
        capture.proxy_lessons,
    );
    if evidence.lessons.is_empty() && evidence.cards.is_empty() {
        return;
    }
    let persist = persist.clone();
    let request_id = capture.request_id.as_u64() as i64;
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        record_evidence(&persist, request_id, session, &evidence).await;
    });
}

pub async fn record_evidence(
    persist: &PersistHandle,
    request_id: i64,
    session: Uuid,
    evidence: &SupplyEvidence,
) {
    record_lessons(persist, request_id, session, &evidence.lessons).await;
    let (proxy, client): (Vec<CarriedCard>, Vec<CarriedCard>) = evidence
        .cards
        .iter()
        .cloned()
        .partition(|card| card.added_by_proxy);
    retry_cards(persist, request_id, session, &client).await;
    record_proxy_cards(persist, request_id, session, &proxy).await;
}

async fn record_lessons(
    persist: &PersistHandle,
    request_id: i64,
    session: Uuid,
    lessons: &[SuppliedLesson],
) {
    let written = persist
        .lessons()
        .record_supplied_lessons(request_id, session, lessons)
        .await;
    if let Err(error) = written {
        tracing::warn!(request_id, %error, "supplied lessons not recorded");
    }
}

async fn record_proxy_cards(
    persist: &PersistHandle,
    request_id: i64,
    session: Uuid,
    cards: &[CarriedCard],
) {
    let written = persist
        .lessons()
        .record_supplied_cards(request_id, session, cards)
        .await;
    match written {
        Ok(rows) if (rows as usize) < cards.len() => {
            tokio::time::sleep(CARD_RECORD_RETRY).await;
            retry_cards(persist, request_id, session, cards).await;
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(request_id, %error, "proxy cards not recorded"),
    }
}

async fn retry_cards(
    persist: &PersistHandle,
    request_id: i64,
    session: Uuid,
    cards: &[CarriedCard],
) {
    let written = persist
        .lessons()
        .record_supplied_cards(request_id, session, cards)
        .await;
    if let Err(error) = written {
        tracing::warn!(request_id, %error, "supplied cards not recorded");
    }
}

#[cfg(test)]
#[path = "supplied_tests.rs"]
pub mod supplied_tests;
