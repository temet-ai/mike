use super::super::PersistError;

pub struct LessonInjectionBackfillCandidate {
    pub request_id: i64,
    pub session_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub lane: String,
    pub persona: Option<String>,
}

pub struct LessonKindTextRow {
    pub id: i64,
    pub kind: String,
    pub text: String,
}

pub struct BackfillInsertRow {
    pub lesson_id: i64,
    pub request_id: i64,
    pub session_id: Option<i64>,
    pub injected_at: chrono::DateTime<chrono::Utc>,
    pub lane: String,
    pub persona: Option<String>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    #[rustfmt::skip]
    pub async fn list_lesson_injection_backfill_candidates(self,
        from: i64,
        to: Option<i64>,
    ) -> Result<Vec<LessonInjectionBackfillCandidate>, PersistError> {
        crate::metrics::time_db("list_lesson_injection_backfill_candidates", async move {
            let to = to.unwrap_or(i64::MAX);
            let mut tx = self.background_pool().begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = 0")
                .execute(&mut *tx)
                .await?;
            let rows = sqlx::query_as!(
                LessonInjectionBackfillCandidate,
                r#"SELECT r.id AS "request_id!",
                          r.session_id,
                          r.created_at AS "created_at!",
                          CASE WHEN torg.code = 'subagent' THEN 'subagent' ELSE 'primary' END AS "lane!",
                          r.subagent_persona AS persona
                     FROM requests.requests r
                     JOIN requests.turn_origins torg ON torg.id = r.turn_origin_id
                    WHERE r.id >= $1
                      AND r.id <= $2
                      AND (
                          (r.sent_patch_bytes IS NOT NULL
                           AND position('proxy-lessons'::bytea IN r.sent_patch_bytes) > 0)
                          OR EXISTS (
                              SELECT 1 FROM requests.deltas d
                               WHERE d.request_id = r.id
                                 AND d.sent_patch_bytes IS NOT NULL
                                 AND position('proxy-lessons'::bytea IN d.sent_patch_bytes) > 0
                          )
                      )
                    ORDER BY r.id"#,
                from,
                to,
            )
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_all_lessons_for_backfill_matching(
        self,
    ) -> Result<Vec<LessonKindTextRow>, PersistError> {
        crate::metrics::time_db("list_all_lessons_for_backfill_matching", async move {
            let rows = sqlx::query_as!(
                LessonKindTextRow,
                r#"SELECT l.id AS "id!", lk.code AS "kind!", l.text AS "text!"
                     FROM memory.lessons l
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    #[rustfmt::skip]
    pub async fn insert_backfilled_lesson_injections(self,
        rows: &[BackfillInsertRow],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let lesson_ids: Vec<i64> = rows.iter().map(|r| r.lesson_id).collect();
        let request_ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();
        let session_ids: Vec<Option<i64>> = rows.iter().map(|r| r.session_id).collect();
        let injected_ats: Vec<chrono::DateTime<chrono::Utc>> =
            rows.iter().map(|r| r.injected_at).collect();
        let lane_codes: Vec<&str> = rows.iter().map(|r| r.lane.as_str()).collect();
        let personas: Vec<Option<&str>> = rows.iter().map(|r| r.persona.as_deref()).collect();
        crate::metrics::time_db("insert_backfilled_lesson_injections", async move {
            let result = sqlx::query!(
                "INSERT INTO memory.lesson_injections
                    (lesson_id, request_id, session_id, injected_at, lane_id, persona)
                 SELECT u.lesson_id, u.request_id, u.session_id, u.injected_at, lane.id, u.persona
                   FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::timestamptz[], $5::text[], $6::text[])
                        AS u(lesson_id, request_id, session_id, injected_at, lane_code, persona)
                   JOIN memory.lesson_injection_lanes lane ON lane.code = u.lane_code
                 ON CONFLICT (lesson_id, request_id) DO NOTHING",
                &lesson_ids,
                &request_ids,
                &session_ids as &[Option<i64>],
                &injected_ats,
                &lane_codes as &[&str],
                &personas as &[Option<&str>],
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
