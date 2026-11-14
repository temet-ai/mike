use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatRequest {
    pub request_id: i64,
    pub seat: String,
    pub session_id: i64,
    pub turn_type: String,
    pub author_kind: String,
    pub turn_origin: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffortRow {
    pub request_id: i64,
    pub seat: String,
    pub session_id: i64,
    pub trigger: String,
    pub label: Option<String>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlabelled {
    pub request_id: i64,
    pub seat: String,
    pub trigger: String,
    pub text: String,
    pub tools: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffortShare {
    pub seat: String,
    pub label: String,
    pub requests: i64,
    pub tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labelled {
    pub request_id: i64,
    pub label: String,
    pub decided_by: String,
    pub record_id: Option<i64>,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn new_seat_requests(
        self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<SeatRequest>, PersistError> {
        crate::metrics::time_db("launch_new_seat_requests", async move {
            let rows = sqlx::query!(
                r#"WITH seats AS (
                       SELECT DISTINCT ON (s.id) l.seat, s.id AS sid, l.created_at AS launched_at
                         FROM launch.launches l
                         JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
                        WHERE l.run_id = $1 AND l.seat IS NOT NULL AND l.seat <> 'status'
                        ORDER BY s.id, l.created_at, l.id
                   ), seen AS (
                       SELECT COALESCE(MAX(request_id), 0) AS last FROM launch.request_efforts WHERE run_id = $1
                   )
                   SELECT q.id AS "request_id!", st.seat AS "seat!", st.sid AS "session_id!",
                          tt.code AS "turn_type!", ak.code AS "author_kind!",
                          orig.code AS "turn_origin!", left(q.human_prompt, 1500) AS "text!"
                     FROM seats st
                     CROSS JOIN seen
                     JOIN requests.requests q ON q.session_id = st.sid AND q.created_at >= st.launched_at
                     JOIN requests.turn_types tt ON tt.id = q.turn_type_id
                     JOIN requests.author_kinds ak ON ak.id = q.author_kind_id
                     JOIN requests.turn_origins orig ON orig.id = q.turn_origin_id
                    WHERE q.id > seen.last
                    ORDER BY q.id
                    LIMIT $2"#,
                run_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| SeatRequest {
                    request_id: r.request_id,
                    seat: r.seat,
                    session_id: r.session_id,
                    turn_type: r.turn_type,
                    author_kind: r.author_kind,
                    turn_origin: r.turn_origin,
                    text: r.text,
                })
                .collect())
        })
        .await
    }

    pub async fn insert_request_efforts(
        self,
        run_id: &str,
        rows: &[EffortRow],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();
        let seats: Vec<String> = rows.iter().map(|r| r.seat.clone()).collect();
        let sessions: Vec<i64> = rows.iter().map(|r| r.session_id).collect();
        let triggers: Vec<String> = rows.iter().map(|r| r.trigger.clone()).collect();
        let labels: Vec<Option<String>> = rows.iter().map(|r| r.label.clone()).collect();
        let by: Vec<Option<String>> = rows.iter().map(|r| r.decided_by.clone()).collect();
        crate::metrics::time_db("launch_insert_request_efforts", async move {
            let done = sqlx::query!(
                "INSERT INTO launch.request_efforts
                     (request_id, run_id, seat, session_id, trigger, label, decided_by, labelled_at)
                 SELECT u.id, $1, u.seat, u.session_id, u.trigger, u.label, u.decided_by,
                        CASE WHEN u.label IS NULL THEN NULL ELSE now() END
                   FROM UNNEST($2::bigint[], $3::text[], $4::bigint[], $5::text[], $6::text[], $7::text[])
                        AS u(id, seat, session_id, trigger, label, decided_by)
                 ON CONFLICT (request_id) DO NOTHING",
                run_id,
                &ids,
                &seats,
                &sessions,
                &triggers,
                &labels as &[Option<String>],
                &by as &[Option<String>],
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn unlabelled_efforts(
        self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<Unlabelled>, PersistError> {
        crate::metrics::time_db("launch_unlabelled_efforts", async move {
            let rows = sqlx::query!(
                r#"SELECT e.request_id AS "request_id!", e.seat AS "seat!", e.trigger AS "trigger!",
                          left(q.human_prompt, 800) AS "text!",
                          COALESCE(t.tools, '') AS "tools!"
                     FROM launch.request_efforts e
                     JOIN requests.requests q ON q.id = e.request_id
                     JOIN responses.responses p ON p.request_id = e.request_id
                     LEFT JOIN LATERAL (
                          SELECT string_agg(
                                     u.tool_name || ': ' || left(COALESCE(u.input->>'command', u.input->>'file_path', u.input->>'pattern', ''), 120),
                                     E'\n' ORDER BY u.id) AS tools
                            FROM tools.uses u
                           WHERE u.request_id = e.request_id
                     ) t ON true
                    WHERE e.run_id = $1 AND e.label IS NULL
                    ORDER BY e.request_id
                    LIMIT $2"#,
                run_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| Unlabelled {
                    request_id: r.request_id,
                    seat: r.seat,
                    trigger: r.trigger,
                    text: r.text,
                    tools: r.tools,
                })
                .collect())
        })
        .await
    }

    pub async fn label_request_efforts(self, labelled: &[Labelled]) -> Result<u64, PersistError> {
        if labelled.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = labelled.iter().map(|l| l.request_id).collect();
        let labels: Vec<String> = labelled.iter().map(|l| l.label.clone()).collect();
        let by: Vec<String> = labelled.iter().map(|l| l.decided_by.clone()).collect();
        let records: Vec<Option<i64>> = labelled.iter().map(|l| l.record_id).collect();
        crate::metrics::time_db("launch_label_request_efforts", async move {
            let done = sqlx::query!(
                "UPDATE launch.request_efforts e
                    SET label = u.label, decided_by = u.decided_by, record_id = u.record_id, labelled_at = now()
                   FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::bigint[])
                        AS u(id, label, decided_by, record_id)
                  WHERE e.request_id = u.id AND e.label IS NULL",
                &ids,
                &labels,
                &by,
                &records as &[Option<i64>],
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn effort_shares(self, run_id: &str) -> Result<Vec<EffortShare>, PersistError> {
        crate::metrics::time_db("launch_effort_shares", async move {
            let rows = sqlx::query!(
                r#"SELECT e.seat AS "seat!", COALESCE(e.label, 'unlabelled') AS "label!",
                          count(*) AS "requests!",
                          COALESCE(sum(p.input_tokens + p.output_tokens), 0)::bigint AS "tokens!"
                     FROM launch.request_efforts e
                     LEFT JOIN responses.responses p ON p.request_id = e.request_id
                    WHERE e.run_id = $1
                    GROUP BY e.seat, COALESCE(e.label, 'unlabelled')
                    ORDER BY e.seat, 2"#,
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| EffortShare {
                    seat: r.seat,
                    label: r.label,
                    requests: r.requests,
                    tokens: r.tokens,
                })
                .collect())
        })
        .await
    }
}
