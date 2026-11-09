use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::WorkThreadError;
use crate::store::persistence::PersistHandle;

const DECISION_LIMIT: i64 = 20;
const NOTE_LIMIT: i64 = 10;
const PROMISE_LIMIT: i64 = 10;
const ASK_LIMIT: i64 = 5;
const PROMISE_EXCERPT_CHARS: i32 = 240;

#[derive(Debug, Clone)]
pub(crate) struct CardThread {
    pub(crate) slug: String,
    pub(crate) title: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) last_session_uuid: Option<Uuid>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardState {
    pub(crate) tldr: String,
    pub(crate) where_we_are: String,
    pub(crate) next_up: String,
    pub(crate) blockers: String,
    pub(crate) open_questions: String,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardEntry {
    pub(crate) id: i64,
    pub(crate) entry_date: NaiveDate,
    pub(crate) session_uuid: Option<Uuid>,
    pub(crate) summary: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CardNote {
    pub(crate) note_uuid: Uuid,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CardPromise {
    pub(crate) id: i64,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) excerpt: String,
    pub(crate) recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct CardAsk {
    pub(crate) operator_turn_id: i64,
    pub(crate) unmet_ask: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct CardInputs {
    pub(crate) thread: CardThread,
    pub(crate) state: Option<CardState>,
    pub(crate) handoff: Option<CardEntry>,
    pub(crate) decisions: Vec<CardEntry>,
    pub(crate) notes: Vec<CardNote>,
    pub(crate) promises: Vec<CardPromise>,
    pub(crate) unmet_asks: Vec<CardAsk>,
}

impl CardInputs {
    pub(crate) fn source_max_at(&self) -> DateTime<Utc> {
        let mut newest = self.thread.updated_at;
        let mut consider = |at: DateTime<Utc>| newest = newest.max(at);
        if let Some(state) = &self.state {
            consider(state.updated_at);
        }
        self.handoff
            .iter()
            .chain(&self.decisions)
            .for_each(|entry| consider(entry.created_at));
        self.notes.iter().for_each(|note| consider(note.updated_at));
        self.promises
            .iter()
            .for_each(|promise| consider(promise.recorded_at));
        self.unmet_asks
            .iter()
            .for_each(|ask| consider(ask.created_at));
        newest
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NewCard<'a> {
    pub(crate) card: &'a Value,
    pub(crate) text: &'a str,
    pub(crate) content_hash: &'a str,
    pub(crate) refs: &'a Value,
    pub(crate) source_max_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct HeadRow {
    slug: String,
    title: Option<String>,
    goal: Option<String>,
    status: Option<String>,
    branch: Option<String>,
    worktree: Option<String>,
    last_session_uuid: Option<Uuid>,
    updated_at: DateTime<Utc>,
    tldr: Option<String>,
    where_we_are: Option<String>,
    next_up: Option<String>,
    blockers: Option<String>,
    open_questions: Option<String>,
    state_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct LogRow {
    id: i64,
    kind: String,
    entry_date: NaiveDate,
    session_uuid: Option<Uuid>,
    summary: String,
    created_at: DateTime<Utc>,
}

impl From<LogRow> for CardEntry {
    fn from(row: LogRow) -> Self {
        Self {
            id: row.id,
            entry_date: row.entry_date,
            session_uuid: row.session_uuid,
            summary: row.summary,
            created_at: row.created_at,
        }
    }
}

fn split_head(row: HeadRow) -> (CardThread, Option<CardState>) {
    let state = row.state_updated_at.map(|updated_at| CardState {
        tldr: row.tldr.unwrap_or_default(),
        where_we_are: row.where_we_are.unwrap_or_default(),
        next_up: row.next_up.unwrap_or_default(),
        blockers: row.blockers.unwrap_or_default(),
        open_questions: row.open_questions.unwrap_or_default(),
        updated_at,
    });
    let thread = CardThread {
        slug: row.slug,
        title: row.title,
        goal: row.goal,
        status: row.status,
        branch: row.branch,
        worktree: row.worktree,
        last_session_uuid: row.last_session_uuid,
        updated_at: row.updated_at,
    };
    (thread, state)
}

async fn head(
    pool: &PgPool,
    thread_id: i64,
) -> Result<Option<(CardThread, Option<CardState>)>, sqlx::Error> {
    let row = sqlx::query_as::<_, HeadRow>(
        "SELECT t.slug, t.title, t.goal, t.status, t.branch, t.worktree, t.last_session_uuid,
                t.updated_at,
                s.tldr, s.where_we_are, s.next_up, s.blockers, s.open_questions,
                s.updated_at AS state_updated_at
           FROM work.threads t
           LEFT JOIN work.thread_state s ON s.thread_id = t.id
          WHERE t.id = $1",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(split_head))
}

async fn log_entries(
    pool: &PgPool,
    thread_id: i64,
) -> Result<(Option<CardEntry>, Vec<CardEntry>), sqlx::Error> {
    let rows = sqlx::query_as::<_, LogRow>(
        "SELECT id, kind, entry_date, session_uuid, summary, created_at
           FROM (
                 SELECT l.id, l.kind, l.entry_date, l.session_uuid, l.summary, l.created_at, l.ord,
                        ROW_NUMBER() OVER (PARTITION BY l.kind ORDER BY l.ord DESC) AS rn,
                        MAX(l.ord) FILTER (WHERE l.kind = 'handoff') OVER () AS handoff_ord
                   FROM work.thread_log l
                  WHERE l.thread_id = $1 AND l.kind IN ('handoff', 'decision')
                ) ranked
          WHERE (kind = 'handoff' AND rn = 1)
             OR (kind = 'decision' AND rn <= $2 AND (handoff_ord IS NULL OR ord > handoff_ord))
          ORDER BY ord DESC",
    )
    .bind(thread_id)
    .bind(DECISION_LIMIT)
    .fetch_all(pool)
    .await?;
    let (handoffs, decisions): (Vec<LogRow>, Vec<LogRow>) =
        rows.into_iter().partition(|row| row.kind == "handoff");
    Ok((
        handoffs.into_iter().next().map(Into::into),
        decisions.into_iter().map(Into::into).collect(),
    ))
}

async fn notes(
    pool: &PgPool,
    thread_id: i64,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<CardNote>, sqlx::Error> {
    sqlx::query_as::<_, CardNote>(
        "SELECT n.note_uuid, n.kind, n.title, n.created_at, n.updated_at
           FROM workspace.session_notes n
           JOIN sessions.sessions s ON s.id = n.session_id
           JOIN work.thread_sessions ts ON ts.session_uuid = s.session_uuid AND ts.thread_id = $1
          WHERE n.kind IN ('decision', 'finding')
            AND ($2::timestamptz IS NULL OR n.created_at > $2)
          ORDER BY n.created_at DESC, n.id DESC
          LIMIT $3",
    )
    .bind(thread_id)
    .bind(since)
    .bind(NOTE_LIMIT)
    .fetch_all(pool)
    .await
}

async fn promises(pool: &PgPool, thread_id: i64) -> Result<Vec<CardPromise>, sqlx::Error> {
    sqlx::query_as::<_, CardPromise>(
        "SELECT d.id, d.operator_turn_id, d.recorded_at,
                right(COALESCE(t.final_assistant_text, ''), $3) AS excerpt
           FROM memory.dossier_entries d
           JOIN sessions.sessions s ON s.id = d.session_id
           JOIN work.thread_sessions ts ON ts.session_uuid = s.session_uuid AND ts.thread_id = $1
           LEFT JOIN memory.operator_turns t ON t.id = d.operator_turn_id
          WHERE d.kind = 'promise' AND d.status = 'open'
          ORDER BY d.effective_at DESC, d.id DESC
          LIMIT $2",
    )
    .bind(thread_id)
    .bind(PROMISE_LIMIT)
    .bind(PROMISE_EXCERPT_CHARS)
    .fetch_all(pool)
    .await
}

async fn unmet_asks(
    pool: &PgPool,
    thread_id: i64,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<CardAsk>, sqlx::Error> {
    sqlx::query_as::<_, CardAsk>(
        "SELECT latest.operator_turn_id, latest.unmet_ask, latest.created_at
           FROM (
                 SELECT DISTINCT ON (v.operator_turn_id)
                        v.operator_turn_id, v.unmet_ask, v.created_at
                   FROM memory.turn_verdicts v
                   JOIN sessions.sessions s ON s.id = v.session_id
                   JOIN work.thread_sessions ts ON ts.session_uuid = s.session_uuid AND ts.thread_id = $1
                  WHERE v.mode = 'live'
                  ORDER BY v.operator_turn_id, v.created_at DESC, v.id DESC
                ) latest
          WHERE latest.unmet_ask <> ''
            AND ($2::timestamptz IS NULL OR latest.created_at > $2)
          ORDER BY latest.created_at DESC, latest.operator_turn_id DESC
          LIMIT $3",
    )
    .bind(thread_id)
    .bind(since)
    .bind(ASK_LIMIT)
    .fetch_all(pool)
    .await
}

async fn gather(pool: &PgPool, thread_id: i64) -> Result<Option<CardInputs>, sqlx::Error> {
    let Some((thread, state)) = head(pool, thread_id).await? else {
        return Ok(None);
    };
    let (handoff, decisions) = log_entries(pool, thread_id).await?;
    let since = handoff.as_ref().map(|entry| entry.created_at);
    Ok(Some(CardInputs {
        thread,
        state,
        handoff,
        decisions,
        notes: notes(pool, thread_id, since).await?,
        promises: promises(pool, thread_id).await?,
        unmet_asks: unmet_asks(pool, thread_id, since).await?,
    }))
}

impl PersistHandle {
    pub(crate) async fn work_thread_card_inputs(
        &self,
        thread_id: i64,
    ) -> Result<Option<CardInputs>, WorkThreadError> {
        Ok(gather(self.pool(), thread_id).await?)
    }

    pub(crate) async fn upsert_work_thread_card(
        &self,
        thread_id: i64,
        card: &NewCard<'_>,
    ) -> Result<bool, WorkThreadError> {
        let written = sqlx::query_scalar::<_, i64>(
            "INSERT INTO work.thread_cards (thread_id, card, text, content_hash, refs, source_max_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (thread_id) DO UPDATE
                SET card = EXCLUDED.card,
                    text = EXCLUDED.text,
                    content_hash = EXCLUDED.content_hash,
                    refs = EXCLUDED.refs,
                    source_max_at = EXCLUDED.source_max_at,
                    built_at = now()
              WHERE work.thread_cards.content_hash IS DISTINCT FROM EXCLUDED.content_hash
             RETURNING thread_id",
        )
        .bind(thread_id)
        .bind(card.card)
        .bind(card.text)
        .bind(card.content_hash)
        .bind(card.refs)
        .bind(card.source_max_at)
        .fetch_optional(self.pool())
        .await?;
        Ok(written.is_some())
    }
}
