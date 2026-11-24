#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::io::BufRead as _;

    use super::super::PersistHandle;
    use crate::store::models::{InterceptedRequest, RequestId, ThreadLink};
    use crate::store::persistence::messages::canonical_clean_messages;
    use crate::store::persistence::reconstruct::rehydrate_body;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[derive(serde::Deserialize)]
    struct Captured {
        id: u64,
        session: String,
        created_at: chrono::DateTime<chrono::Utc>,
        turn_origin: String,
        predecessor: Option<u64>,
        rule: Option<String>,
        body: String,
    }

    #[derive(Default, Debug)]
    struct SessionTally {
        distinct: HashSet<String>,
        stored: i64,
    }

    fn replayed(captured: &Captured) -> InterceptedRequest {
        let mut req = crate::store::tests::make_test_request(
            Some(RequestId::from_raw(captured.id)),
            &captured.session,
        );
        req.created_at = captured.created_at;
        req.turn_origin = captured.turn_origin.clone();
        req.request_body = Some(bytes::Bytes::from(captured.body.clone()));
        req.thread = ThreadLink::from_body(captured.body.as_bytes()).map(|mut link| {
            if captured.rule.as_deref() == Some("thread_link") {
                link.predecessor = captured.predecessor.map(RequestId::from_raw);
            }
            link
        });
        req
    }

    async fn rebuilt(handle: &PersistHandle, id: u64) -> anyhow::Result<Option<bytes::Bytes>> {
        let id = i64::try_from(id)?;
        let (body, envelope): (Option<Vec<u8>>, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT request_body, request_envelope FROM requests.requests WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&handle.pool)
        .await?;
        Ok(rehydrate_body(
            &handle.pool,
            id,
            body.map(bytes::Bytes::from),
            envelope.map(bytes::Bytes::from),
        )
        .await?
        .map(|served| served.bytes))
    }

    async fn stored_per_session(handle: &PersistHandle) -> anyhow::Result<HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT s.session_uuid::text, count(DISTINCT rm.message_id)
               FROM requests.messages rm
               JOIN requests.requests r ON r.id = rm.request_id
               JOIN sessions.sessions s ON s.id = r.session_id
              GROUP BY 1",
        )
        .fetch_all(&handle.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    async fn repeats_by_rule(handle: &PersistHandle) -> anyhow::Result<Vec<(String, String, i64)>> {
        Ok(sqlx::query_as(
            "WITH first_use AS (
                 SELECT DISTINCT ON (rm.message_id) rm.message_id, rm.request_id
                   FROM requests.messages rm ORDER BY rm.message_id, rm.request_id),
             copies AS (
                 SELECT coalesce(pr.code, 'none') AS rule, o.code AS origin,
                        row_number() OVER (PARTITION BY r.session_id, m.message_sha256 ORDER BY m.id) AS copy
                   FROM first_use f
                   JOIN messages.messages m ON m.id = f.message_id
                   JOIN requests.requests r ON r.id = f.request_id
                   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                   LEFT JOIN requests.predecessor_rules pr ON pr.id = r.predecessor_rule_id)
             SELECT rule, origin, count(*) FROM copies WHERE copy > 1 GROUP BY 1, 2 ORDER BY 3 DESC",
        )
        .fetch_all(&handle.pool)
        .await?)
    }

    #[tokio::test]
    async fn replayed_capture_stores_each_message_once_and_rebuilds_every_body()
    -> anyhow::Result<()> {
        let Ok(path) = std::env::var("MIKE_REPLAY_FILE") else {
            return Ok(());
        };
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let mut captured = Vec::new();
        for line in std::io::BufReader::new(std::fs::File::open(path)?).lines() {
            captured.push(serde_json::from_str::<Captured>(&line?)?);
        }

        let mut tallies: HashMap<String, SessionTally> = HashMap::new();
        for request in &captured {
            let fresh_session = !tallies.contains_key(&request.session);
            let tally = tallies.entry(request.session.clone()).or_default();
            for message in canonical_clean_messages(request.body.as_bytes())? {
                tally.distinct.insert(message.sha);
            }
            handle
                .insert_request_initial(&replayed(request), fresh_session, IdMode::Ensure)
                .await?;
        }

        let mut mismatched = Vec::new();
        for request in &captured {
            if rebuilt(&handle, request.id).await?.as_deref() != Some(request.body.as_bytes()) {
                mismatched.push(request.id);
            }
        }
        for (session, stored) in stored_per_session(&handle).await? {
            if let Some(tally) = tallies.get_mut(&session) {
                tally.stored = stored;
            }
        }
        let stored: i64 = tallies.values().map(|t| t.stored).sum();
        let distinct = tallies.values().map(|t| t.distinct.len()).sum::<usize>();
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM messages.messages")
            .fetch_one(&handle.pool)
            .await?;
        eprintln!(
            "replay: {} requests, {} distinct messages per session, {} stored rows, {} referenced, {} bodies differ",
            captured.len(),
            distinct,
            total,
            stored,
            mismatched.len()
        );
        eprintln!(
            "replay repeats by rule: {:?}",
            repeats_by_rule(&handle).await?
        );
        assert!(
            mismatched.is_empty(),
            "bodies that rebuild differently: {mismatched:?}"
        );
        assert!(
            total <= i64::try_from(distinct)? * 103 / 100,
            "{total} rows stored for {distinct} distinct messages"
        );
        db.cleanup().await?;
        Ok(())
    }
}
