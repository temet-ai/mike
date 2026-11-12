use serde::Serialize;

use crate::store::persistence::{PersistError, PersistHandle};
use crate::wire::genuine::{CLASSIFIER_VERSION, CaptureRule, PromptClass};

pub struct ClassificationWrite<'a> {
    pub request_id: i64,
    pub session_db_id: i64,
    pub kind_code: &'a str,
    pub rule: Option<&'a str>,
    pub repeat_key: Option<&'a str>,
    pub classifier_version: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReclassifyRow {
    pub request_id: i64,
    pub session_id: i64,
    pub kind: String,
    pub rule: Option<String>,
    pub repeat_key: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclassifyOutcome {
    pub updated: u64,
    pub inserted: u64,
}

pub fn capture_rule_code(rule: CaptureRule) -> &'static str {
    match rule {
        CaptureRule::A => "a",
        CaptureRule::C => "c",
        CaptureRule::Envelope => "envelope",
    }
}

impl PersistHandle {
    pub async fn write_prompt_classification(
        &self,
        request_id: i64,
        session_db_id: i64,
        class: &PromptClass,
        repeat_key: Option<&str>,
    ) -> Result<(), PersistError> {
        self.write_classification_row(&ClassificationWrite {
            request_id,
            session_db_id,
            kind_code: class.kind.stored_code(),
            rule: class.rule.map(capture_rule_code),
            repeat_key,
            classifier_version: CLASSIFIER_VERSION,
        })
        .await
    }

    pub async fn write_classification_row(
        &self,
        row: &ClassificationWrite<'_>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("write_prompt_classification", async move {
            let kind_found = sqlx::query_scalar!(
                r#"WITH k AS (
                     SELECT id FROM prompt_kinds.kinds WHERE code = $3
                 ), ins AS (
                     INSERT INTO prompt_kinds.classifications
                            (request_id, session_id, kind_id, rule, repeat_key, classifier_version)
                     SELECT $1, $2, k.id, $4, $5, $6 FROM k
                     ON CONFLICT (request_id) DO NOTHING
                     RETURNING 1
                 )
                 SELECT count(*) AS "n!" FROM k"#,
                row.request_id,
                row.session_db_id,
                row.kind_code,
                row.rule,
                row.repeat_key,
                row.classifier_version,
            )
            .fetch_one(&self.pool)
            .await?;
            if kind_found == 0 {
                return Err(PersistError::DataInvariant(format!(
                    "prompt_kinds.kinds has no code {:?}",
                    row.kind_code
                )));
            }
            Ok(())
        })
        .await
    }

    pub async fn reclassify_batch(
        &self,
        rows: &[ReclassifyRow],
    ) -> Result<ReclassifyOutcome, PersistError> {
        if rows.is_empty() {
            return Ok(ReclassifyOutcome {
                updated: 0,
                inserted: 0,
            });
        }
        let payload = serde_json::to_value(rows)?;
        crate::metrics::time_db("reclassify_batch", async move {
            let mut tx = self.pool.begin().await?;
            let updated = sqlx::query!(
                "UPDATE prompt_kinds.classifications c
                    SET kind_id = k.id,
                        rule = x.rule,
                        repeat_key = x.repeat_key,
                        classifier_version = x.version
                   FROM jsonb_to_recordset($1::jsonb)
                        AS x(request_id bigint, session_id bigint, kind text,
                             rule text, repeat_key text, version text)
                   JOIN prompt_kinds.kinds k ON k.code = x.kind
                  WHERE c.request_id = x.request_id
                    AND (c.kind_id, c.rule, c.repeat_key, c.classifier_version)
                        IS DISTINCT FROM (k.id, x.rule, x.repeat_key, x.version)",
                &payload,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let inserted = sqlx::query!(
                "INSERT INTO prompt_kinds.classifications
                        (request_id, session_id, kind_id, rule, repeat_key, classifier_version)
                 SELECT x.request_id, x.session_id, k.id, x.rule, x.repeat_key, x.version
                   FROM jsonb_to_recordset($1::jsonb)
                        AS x(request_id bigint, session_id bigint, kind text,
                             rule text, repeat_key text, version text)
                   JOIN prompt_kinds.kinds k ON k.code = x.kind
                 ON CONFLICT (request_id) DO NOTHING",
                &payload,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            Ok(ReclassifyOutcome { updated, inserted })
        })
        .await
    }
}
