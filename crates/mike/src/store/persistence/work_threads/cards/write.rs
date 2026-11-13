use chrono::{DateTime, Utc};
use serde_json::Value;

use super::super::WorkThreadError;
use super::NewCard;

struct Columns {
    thread_ids: Vec<i64>,
    cards: Vec<Value>,
    texts: Vec<String>,
    hashes: Vec<String>,
    refs: Vec<Value>,
    source_max_at: Vec<DateTime<Utc>>,
}

fn columns(cards: &[NewCard]) -> Columns {
    Columns {
        thread_ids: cards.iter().map(|card| card.thread_id).collect(),
        cards: cards.iter().map(|card| card.card.clone()).collect(),
        texts: cards.iter().map(|card| card.text.clone()).collect(),
        hashes: cards.iter().map(|card| card.content_hash.clone()).collect(),
        refs: cards.iter().map(|card| card.refs.clone()).collect(),
        source_max_at: cards.iter().map(|card| card.source_max_at).collect(),
    }
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn write_work_thread_cards(
        self,
        cards: &[NewCard],
    ) -> Result<usize, WorkThreadError> {
        if cards.is_empty() {
            return Ok(0);
        }
        let cols = columns(cards);
        let mut tx = self.pool().begin().await?;
        let written = sqlx::query_scalar!(
            r#"INSERT INTO work.thread_cards (thread_id, card, text, content_hash, refs, source_max_at)
               SELECT u.thread_id, u.card, u.text, u.content_hash, u.refs, u.source_max_at
                 FROM UNNEST($1::bigint[], $2::jsonb[], $3::text[], $4::text[], $5::jsonb[],
                             $6::timestamptz[])
                      AS u(thread_id, card, text, content_hash, refs, source_max_at)
               ON CONFLICT (thread_id) DO UPDATE
                  SET card = EXCLUDED.card,
                      text = EXCLUDED.text,
                      content_hash = EXCLUDED.content_hash,
                      refs = EXCLUDED.refs,
                      built_at = now()
                WHERE work.thread_cards.content_hash IS DISTINCT FROM EXCLUDED.content_hash
               RETURNING thread_id"#,
            &cols.thread_ids,
            &cols.cards,
            &cols.texts,
            &cols.hashes,
            &cols.refs,
            &cols.source_max_at,
        )
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query!(
            r#"UPDATE work.thread_cards c
                  SET source_max_at = u.source_max_at
                 FROM UNNEST($1::bigint[], $2::timestamptz[]) AS u(thread_id, source_max_at)
                WHERE c.thread_id = u.thread_id AND c.source_max_at < u.source_max_at"#,
            &cols.thread_ids,
            &cols.source_max_at,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(written.len())
    }
}
