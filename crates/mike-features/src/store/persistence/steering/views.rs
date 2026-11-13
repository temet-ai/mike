use super::{
    ArmingHistoryRow, PriorFireRow, SessionSteeringRow, SteeredRequestRow, SteeringVocabulary,
    VerifierRateRow, VocabTermRow,
};
use crate::store::persistence::PersistError;

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn fetch_steered_request(
        self,
        request_id: i64,
    ) -> Result<Option<SteeredRequestRow>, PersistError> {
        crate::metrics::time_db("fetch_steered_request", async move {
            let row = sqlx::query_as!(
                SteeredRequestRow,
                r#"SELECT
                       request_id,
                       response_id,
                       session_uuid,
                       operator_turn_id,
                       steerer_name,
                       steerer_display_name,
                       authority,
                       mode,
                       verdict,
                       off_thread_topic,
                       confidence,
                       payload_text,
                       anchor_text,
                       verdict_json,
                       model,
                       sent_at,
                       processed_at,
                       expires_at,
                       reverted_at,
                       revert_reason,
                       created_at,
                       trigger_request_id
                   FROM mike.steered_requests_v
                  WHERE request_id = $1"#,
                request_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn fetch_session_steering(
        self,
        session_uuid: uuid::Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<SessionSteeringRow>, PersistError> {
        let rows = sqlx::query_as!(
            SessionSteeringRow,
            r#"SELECT
                   intervention_id,
                   session_id,
                   session_uuid,
                   project_path,
                   steerer_name,
                   steerer_display_name,
                   verdict,
                   authority,
                   mode,
                   trigger_request_id,
                   sent_request_id,
                   sent_response_id,
                   operator_turn_id,
                   gate_reason,
                   off_thread_topic,
                   confidence,
                   anchor_text,
                   verdict_json,
                   payload_text,
                   model,
                   sent_at,
                   processed_at,
                   expires_at,
                   reverted_at,
                   revert_reason,
                   created_at
               FROM mike.session_steering_v
              WHERE session_uuid = $1
              ORDER BY created_at DESC
              OFFSET $2 LIMIT $3"#,
            session_uuid,
            offset,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn count_session_steering(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<i64, PersistError> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\"
               FROM mike.session_steering_v
              WHERE session_uuid = $1",
            session_uuid,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    pub async fn get_session_project_id(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("get_session_project_id", async move {
            Ok(sqlx::query_scalar!(
                "SELECT project_id FROM sessions.sessions WHERE session_uuid = $1",
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_verifier_counts_for_session(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<(i64, i64), PersistError> {
        crate::metrics::time_db("fetch_verifier_counts_for_session", async move {
            let row = sqlx::query!(
                r#"SELECT count(*) AS "fired!", count(i.delivered_request_id) AS "delivered!"
                     FROM steering.interventions i
                     JOIN steering.steerers s2 ON s2.id = i.steerer_id AND s2.kind = 'verifier'
                     JOIN sessions.sessions s ON s.id = i.session_id
                    WHERE s.session_uuid = $1"#,
                session_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((row.fired, row.delivered))
        })
        .await
    }

    pub async fn fetch_verifier_rate(
        self,
        project: Option<&str>,
    ) -> Result<Vec<VerifierRateRow>, PersistError> {
        let project = project.map(str::trim).filter(|s| !s.is_empty());
        crate::metrics::time_db("fetch_verifier_rate", async move {
            let rows = match project {
                Some(project) => {
                    sqlx::query_as!(
                        VerifierRateRow,
                        r#"SELECT
                               project_path     AS "project_path!",
                               fires_count      AS "fires_count!: i64",
                               corrected_count  AS "corrected_count!: i64",
                               delivered_count  AS "delivered_count!: i64",
                               excluded_judge_failed_count AS "excluded_judge_failed_count!: i64",
                               corrected_rate,
                               delivered_rate
                           FROM steering.verifier_rate_v
                           WHERE project_path ILIKE '%' || $1 || '%'
                           ORDER BY fires_count DESC"#,
                        project,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_as!(
                        VerifierRateRow,
                        r#"SELECT
                               project_path     AS "project_path!",
                               fires_count      AS "fires_count!: i64",
                               corrected_count  AS "corrected_count!: i64",
                               delivered_count  AS "delivered_count!: i64",
                               excluded_judge_failed_count AS "excluded_judge_failed_count!: i64",
                               corrected_rate,
                               delivered_rate
                           FROM steering.verifier_rate_v
                           ORDER BY fires_count DESC"#,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_arming_history(
        self,
        session_db_id: i64,
    ) -> Result<ArmingHistoryRow, PersistError> {
        const ARMING_HISTORY_RECENT_CAP: i64 = 20;

        crate::metrics::time_db("fetch_arming_history", async move {
            let row = sqlx::query!(
                r#"WITH closed AS (
                       SELECT t.id, tot.code
                         FROM memory.operator_turns t
                         JOIN memory.turn_outcomes  tot ON tot.id = t.outcome_id
                        WHERE t.session_id = $1 AND t.closed_at IS NOT NULL
                   ), recent AS (
                       SELECT code FROM closed ORDER BY id DESC LIMIT $2
                   )
                   SELECT
                       (SELECT COUNT(*) FROM closed)::bigint AS "closed_turn_count!",
                       COALESCE(
                           (SELECT array_agg(code) FROM recent),
                           ARRAY[]::text[]
                       ) AS "recent_outcomes_newest_first!: Vec<String>""#,
                session_db_id,
                ARMING_HISTORY_RECENT_CAP,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(ArmingHistoryRow {
                closed_turn_count: row.closed_turn_count,
                recent_outcomes_newest_first: row.recent_outcomes_newest_first,
            })
        })
        .await
    }

    pub async fn fetch_prior_fires_for_session(
        self,
        session_db_id: i64,
        steerer_id: i64,
        limit: i64,
    ) -> Result<Vec<PriorFireRow>, PersistError> {
        crate::metrics::time_db("fetch_prior_fires_for_session", async move {
            let rows = sqlx::query_as!(
                PriorFireRow,
                r#"SELECT off_thread_topic, created_at
                     FROM steering.interventions
                    WHERE session_id = $1 AND steerer_id = $2
                    ORDER BY created_at DESC
                    LIMIT $3"#,
                session_db_id,
                steerer_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_steering_vocabulary(
        self,
    ) -> Result<SteeringVocabulary, PersistError> {
        let rows = sqlx::query!(
            r#"SELECT 'authority' AS "kind!", code AS "code!", description AS "description!"
                 FROM steering.authorities
               UNION ALL
               SELECT 'mode', code, description FROM steering.modes
               UNION ALL
               SELECT 'verdict', code, description FROM steering.verdicts
               ORDER BY 1, 2"#,
        )
        .fetch_all(&self.pool)
        .await?;
        let mut vocabulary = SteeringVocabulary {
            authorities: Vec::new(),
            modes: Vec::new(),
            verdicts: Vec::new(),
        };
        for row in rows {
            let term = VocabTermRow {
                code: row.code,
                description: row.description,
            };
            match row.kind.as_str() {
                "authority" => vocabulary.authorities.push(term),
                "mode" => vocabulary.modes.push(term),
                _ => vocabulary.verdicts.push(term),
            }
        }
        Ok(vocabulary)
    }
}
