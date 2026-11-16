use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::steerer::criteria_source_point::{ACCEPT_AT, Asks, POINT, QUESTION_SET, key, state};
use crate::steerer::fleet::verifier_criteria::split_asks;
use crate::steerer::verifier::ask_ledger::written_by_the_operator;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use mike_fleet::store::persistence::work_threads::criteria::ThreadCriteria;

const OPENING: i64 = 2;
const NEWEST: i64 = 30;
const OPENING_LEN: usize = 2;
const NEWEST_LEN: usize = 30;

const ASKS_AS_OF: &str = "SELECT * FROM ( \
        (SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at \
           FROM memory.ask_ledger_entries e \
           LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id \
           LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id \
          WHERE e.session_id = $1 AND e.created_at <= $2 \
            AND (k.code IS NULL OR k.code = 'operator') \
          ORDER BY e.created_at ASC LIMIT $3) \
        UNION ALL \
        (SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at \
           FROM memory.ask_ledger_entries e \
           LEFT JOIN prompt_kinds.classifications c ON c.request_id = e.request_id \
           LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id \
          WHERE e.session_id = $1 AND e.created_at <= $2 \
            AND (k.code IS NULL OR k.code = 'operator') \
          ORDER BY e.created_at DESC LIMIT $4) \
     ) both_ends ORDER BY created_at ASC";

pub(super) struct Ready {
    pub record: Labelled,
    pub threads: Vec<ThreadCriteria>,
    pub asks: Asks,
    pub expected: Vec<bool>,
}

pub(super) struct CriteriaSource;

fn stored_threads(record: &Labelled) -> Option<Vec<ThreadCriteria>> {
    let threads = record.state.get("threads")?.as_object()?;
    (0..threads.len())
        .map(|i| {
            let t = threads.get(&key(i))?;
            let text = |name: &str| t.get(name).and_then(Value::as_str).map(str::to_owned);
            Some(ThreadCriteria {
                thread_slug: text("slug")?,
                title: text("title").unwrap_or_default(),
                criteria_text: text("criteria")?,
            })
        })
        .collect()
}

fn expected(record: &Labelled, count: usize) -> Option<Vec<bool>> {
    (0..count)
        .map(|i| {
            let question = key(i);
            match record.expected_for(&question).and_then(Value::as_str) {
                Some(want) => Some(want == "yes"),
                None => record.old_noul(&question).map(|p| p >= ACCEPT_AT),
            }
        })
        .collect()
}

async fn asks_as_of(persist: &PersistHandle, record: &Labelled) -> Option<Asks> {
    let mut rows = sqlx::query_as::<_, AskLedgerEntryRow>(ASKS_AS_OF)
        .bind(record.session_id?)
        .bind(record.created_at)
        .bind(OPENING)
        .bind(NEWEST)
        .fetch_all(persist.pool())
        .await
        .ok()?;
    rows.sort_by_key(|r| (r.created_at, r.id));
    rows.dedup_by_key(|r| r.id);
    let opening = rows[..rows.len().min(OPENING_LEN)].to_vec();
    let newest = rows[rows.len().saturating_sub(NEWEST_LEN)..].to_vec();
    Some(split_asks(
        &written_by_the_operator(opening),
        &written_by_the_operator(newest),
    ))
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let records = labelled::load(persist, POINT, since, limit).await?;
    let total = records.len();
    let mut out = Vec::with_capacity(total);
    for record in records {
        let Some(threads) = stored_threads(&record) else {
            continue;
        };
        let Some(expected) = expected(&record, threads.len()) else {
            continue;
        };
        let Some(asks) = asks_as_of(persist, &record).await else {
            continue;
        };
        out.push(Ready {
            record,
            threads,
            asks,
            expected,
        });
    }
    let skipped = total - out.len();
    Ok((out, skipped))
}

impl ReplayPoint for CriteriaSource {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = labelled::SOURCE;

    fn item_id(item: &Ready) -> String {
        item.record.record_id.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.record.created_at
    }

    fn label(item: &Ready) -> String {
        format!("{} expected={:?}", item.record.label, item.expected)
    }

    fn prepare(item: &Ready) -> Prepared {
        Prepared {
            state: state(&item.threads, &item.asks),
            questions: crate::steerer::criteria_source_point::questions::questions(
                item.threads.len(),
            ),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let taken: Vec<bool> = (0..item.threads.len())
            .map(|i| asked.noul(&key(i)).map(|p| p >= ACCEPT_AT))
            .collect::<Option<_>>()?;
        let right = taken == item.expected;
        let score = (0..item.threads.len())
            .filter_map(|i| asked.noul(&key(i)))
            .fold(0.0_f64, f64::max);
        Some(Judged {
            composed: json!({ "taken": taken, "expected": item.expected }),
            score,
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: Some(item.record.right() == right),
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        let index = (0..item.threads.len()).find(|i| key(*i) == question)?;
        item.expected.get(index).copied()
    }

    fn current_positive(item: &Ready, question: &str) -> Option<bool> {
        item.record.old_noul(question).map(|p| p >= ACCEPT_AT)
    }
}
