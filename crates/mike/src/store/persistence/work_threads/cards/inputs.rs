use std::collections::HashMap;

use sqlx::PgPool;

use super::super::WorkThreadError;
use super::super::reads::{ThreadStateRow, identities_by_ids, states_by_ids};
use super::related::related_items;
use super::{
    ASK_CAP, CardAsk, CardEntry, CardInputs, CardNote, CardPromise, CardRelated, DECISION_CAP,
    NOTE_CAP, PROMISE_CAP, RECENT_DAYS,
};
use crate::store::persistence::PersistHandle;

async fn log_entries(pool: &PgPool, thread_ids: &[i64]) -> Result<Vec<CardEntry>, sqlx::Error> {
    sqlx::query_as!(
        CardEntry,
        r#"SELECT r.thread_id AS "thread_id!", r.id AS "id!", r.kind AS "kind!",
                  r.entry_date AS "entry_date!", r.session_uuid AS "session_uuid?",
                  r.summary AS "summary!"
             FROM (
                   SELECT l.thread_id, l.id, l.kind, l.entry_date, l.session_uuid, l.summary, l.ord,
                          row_number() OVER (PARTITION BY l.thread_id, l.kind ORDER BY l.ord DESC) AS rn,
                          max(l.ord) FILTER (WHERE l.kind = 'handoff')
                              OVER (PARTITION BY l.thread_id) AS handoff_ord
                     FROM work.thread_log l
                    WHERE l.thread_id = ANY($1) AND l.kind IN ('handoff', 'decision')
                  ) r
            WHERE (r.kind = 'handoff' AND r.rn = 1)
               OR (r.kind = 'decision' AND r.rn <= $2
                   AND (r.handoff_ord IS NULL OR r.ord > r.handoff_ord))
            ORDER BY r.thread_id, r.ord DESC"#,
        thread_ids,
        DECISION_CAP,
    )
    .fetch_all(pool)
    .await
}

async fn tagged_notes(pool: &PgPool, thread_ids: &[i64]) -> Result<Vec<CardNote>, sqlx::Error> {
    sqlx::query_as!(
        CardNote,
        r#"WITH handoffs AS (
               SELECT DISTINCT ON (l.thread_id) l.thread_id, l.entry_date
                 FROM work.thread_log l
                WHERE l.thread_id = ANY($1) AND l.kind = 'handoff'
                ORDER BY l.thread_id, l.ord DESC
           )
           SELECT r.thread_id AS "thread_id!", r.note_uuid AS "note_uuid!", r.kind AS "kind!",
                  r.title AS "title!", r.created_at AS "created_at!"
             FROM (
                   SELECT t.id AS thread_id, n.note_uuid, n.kind, n.title, n.created_at,
                          row_number() OVER (PARTITION BY t.id ORDER BY n.created_at DESC, n.id DESC) AS rn
                     FROM work.threads t
                     LEFT JOIN handoffs h ON h.thread_id = t.id
                     JOIN workspace.session_notes n ON n.tags @> ARRAY['thread:' || t.slug]
                     JOIN sessions.sessions se ON se.id = n.session_id AND se.project_id = t.project_id
                    WHERE t.id = ANY($1)
                      AND n.kind IN ('decision', 'finding')
                      AND (h.entry_date IS NULL
                           OR n.created_at >= h.entry_date::timestamp AT TIME ZONE 'UTC')
                  ) r
            WHERE r.rn <= $2
            ORDER BY r.thread_id, r.created_at DESC"#,
        thread_ids,
        NOTE_CAP,
    )
    .fetch_all(pool)
    .await
}

async fn open_promises(pool: &PgPool, thread_ids: &[i64]) -> Result<Vec<CardPromise>, sqlx::Error> {
    sqlx::query_as!(
        CardPromise,
        r#"WITH handoffs AS (
               SELECT DISTINCT ON (l.thread_id) l.thread_id, l.entry_date
                 FROM work.thread_log l
                WHERE l.thread_id = ANY($1) AND l.kind = 'handoff'
                ORDER BY l.thread_id, l.ord DESC
           ),
           bounds AS (
               SELECT t.id AS thread_id,
                      GREATEST(now() - make_interval(days => $3),
                               h.entry_date::timestamp AT TIME ZONE 'UTC',
                               st.updated_at) AS since
                 FROM work.threads t
                 LEFT JOIN handoffs h ON h.thread_id = t.id
                 LEFT JOIN work.thread_state st ON st.thread_id = t.id
                WHERE t.id = ANY($1)
           )
           SELECT r.thread_id AS "thread_id!", r.id AS "id!",
                  r.operator_turn_id AS "operator_turn_id?", r.text AS "text?",
                  r.effective_at AS "effective_at!"
             FROM (
                   SELECT b.thread_id, d.id, d.operator_turn_id, d.effective_at,
                          CASE WHEN ((d.data -> 'span') ->> 0) ~ '^[0-9]{1,9}$'
                                AND ((d.data -> 'span') ->> 1) ~ '^[0-9]{1,9}$'
                               THEN regexp_replace(
                                        substring(ot.final_assistant_text
                                          FROM GREATEST(((d.data -> 'span') ->> 0)::int - 299, 1)
                                          FOR LEAST(((d.data -> 'span') ->> 0)::int, 300)),
                                        '^.*[.!?\n][[:space:]]*', '')
                                 || substring(ot.final_assistant_text
                                      FROM ((d.data -> 'span') ->> 0)::int + 1
                                      FOR GREATEST(((d.data -> 'span') ->> 1)::int
                                                   - ((d.data -> 'span') ->> 0)::int, 0))
                                 || COALESCE(substring(substring(ot.final_assistant_text
                                                FROM ((d.data -> 'span') ->> 1)::int + 1 FOR 200)
                                              FROM '^[^.!?\n]*[.!?]?'), '')
                          END AS text,
                          row_number() OVER (PARTITION BY b.thread_id
                                             ORDER BY d.effective_at DESC, d.id DESC) AS rn
                     FROM bounds b
                     JOIN work.thread_sessions ts ON ts.thread_id = b.thread_id
                     JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
                     JOIN memory.dossier_entries d
                       ON d.session_id = se.id AND d.kind = 'promise' AND d.status = 'open'
                      AND d.effective_at > b.since
                     LEFT JOIN memory.operator_turns ot ON ot.id = d.operator_turn_id
                  ) r
            WHERE r.rn <= $2
            ORDER BY r.thread_id, r.effective_at DESC"#,
        thread_ids,
        PROMISE_CAP,
        RECENT_DAYS,
    )
    .fetch_all(pool)
    .await
}

async fn unmet_asks(pool: &PgPool, thread_ids: &[i64]) -> Result<Vec<CardAsk>, sqlx::Error> {
    sqlx::query_as!(
        CardAsk,
        r#"WITH handoffs AS (
               SELECT DISTINCT ON (l.thread_id) l.thread_id, l.entry_date
                 FROM work.thread_log l
                WHERE l.thread_id = ANY($1) AND l.kind = 'handoff'
                ORDER BY l.thread_id, l.ord DESC
           ),
           bounds AS (
               SELECT t.id AS thread_id,
                      GREATEST(now() - make_interval(days => $3),
                               h.entry_date::timestamp AT TIME ZONE 'UTC',
                               st.updated_at) AS since
                 FROM work.threads t
                 LEFT JOIN handoffs h ON h.thread_id = t.id
                 LEFT JOIN work.thread_state st ON st.thread_id = t.id
                WHERE t.id = ANY($1)
           )
           SELECT r.thread_id AS "thread_id!", r.operator_turn_id AS "operator_turn_id!",
                  r.unmet_ask AS "unmet_ask!", r.created_at AS "created_at!"
             FROM (
                   SELECT latest.thread_id, latest.operator_turn_id, latest.unmet_ask, latest.created_at,
                          row_number() OVER (PARTITION BY latest.thread_id
                                             ORDER BY latest.created_at DESC,
                                                      latest.operator_turn_id DESC) AS rn
                     FROM (
                           SELECT DISTINCT ON (b.thread_id, v.operator_turn_id)
                                  b.thread_id, v.operator_turn_id, v.unmet_ask, v.created_at
                             FROM bounds b
                             JOIN work.thread_sessions ts ON ts.thread_id = b.thread_id
                             JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
                             JOIN memory.turn_verdicts v
                               ON v.session_id = se.id AND v.mode = 'live' AND v.created_at > b.since
                            ORDER BY b.thread_id, v.operator_turn_id, v.created_at DESC, v.id DESC
                          ) latest
                    WHERE btrim(latest.unmet_ask) <> ''
                  ) r
            WHERE r.rn <= $2
            ORDER BY r.thread_id, r.created_at DESC"#,
        thread_ids,
        ASK_CAP,
        RECENT_DAYS,
    )
    .fetch_all(pool)
    .await
}

fn group<T>(rows: Vec<T>, key: impl Fn(&T) -> i64) -> HashMap<i64, Vec<T>> {
    let mut grouped: HashMap<i64, Vec<T>> = HashMap::new();
    for row in rows {
        grouped.entry(key(&row)).or_default().push(row);
    }
    grouped
}

struct Gathered {
    states: HashMap<i64, ThreadStateRow>,
    log: HashMap<i64, Vec<CardEntry>>,
    notes: HashMap<i64, Vec<CardNote>>,
    promises: HashMap<i64, Vec<CardPromise>>,
    asks: HashMap<i64, Vec<CardAsk>>,
    related: HashMap<i64, Vec<CardRelated>>,
}

impl Gathered {
    fn inputs_for(&mut self, thread: super::super::ThreadIdentity) -> CardInputs {
        let id = thread.id;
        let (handoffs, decisions): (Vec<CardEntry>, Vec<CardEntry>) = self
            .log
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .partition(|entry| entry.kind == "handoff");
        CardInputs {
            thread,
            state: self.states.remove(&id).map(|row| row.state),
            handoff: handoffs.into_iter().next(),
            decisions,
            notes: self.notes.remove(&id).unwrap_or_default(),
            promises: self.promises.remove(&id).unwrap_or_default(),
            unmet_asks: self.asks.remove(&id).unwrap_or_default(),
            related: self.related.remove(&id).unwrap_or_default(),
        }
    }
}

impl PersistHandle {
    pub(crate) async fn work_thread_card_inputs(
        &self,
        thread_ids: &[i64],
    ) -> Result<Vec<CardInputs>, WorkThreadError> {
        let pool = self.pool();
        let (threads, states, log, notes, promises, asks, related) = tokio::try_join!(
            identities_by_ids(pool, thread_ids),
            states_by_ids(pool, thread_ids),
            log_entries(pool, thread_ids),
            tagged_notes(pool, thread_ids),
            open_promises(pool, thread_ids),
            unmet_asks(pool, thread_ids),
            related_items(pool, thread_ids),
        )?;
        let mut gathered = Gathered {
            states: states.into_iter().map(|row| (row.thread_id, row)).collect(),
            log: group(log, |entry| entry.thread_id),
            notes: group(notes, |note| note.thread_id),
            promises: group(promises, |promise| promise.thread_id),
            asks: group(asks, |ask| ask.thread_id),
            related: group(related, |item| item.thread_id),
        };
        Ok(threads
            .into_iter()
            .map(|thread| gathered.inputs_for(thread))
            .collect())
    }
}
