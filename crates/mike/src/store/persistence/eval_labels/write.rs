use serde_json::{Value, json};
use sqlx::PgConnection;

use super::super::PersistError;
use super::EvalLabelRow;
use crate::eval::labels::SpaOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Turn { turn_id: i64 },
    SessionEnd { session_uuid: uuid::Uuid },
}

impl Subject {
    fn kind(&self) -> &'static str {
        match self {
            Subject::Turn { .. } => "turn",
            Subject::SessionEnd { .. } => "session_end",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GoldEntry {
    pub axis: String,
    pub label: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LabelWriteError {
    #[error("persistence error: {0}")]
    Persist(#[from] PersistError),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Open(String),
    #[error("label changed; reload")]
    Conflict,
    #[error("{0}")]
    Invalid(String),
}

impl From<sqlx::Error> for LabelWriteError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e
            && db.constraint() == Some("labels_single_successor")
        {
            return LabelWriteError::Conflict;
        }
        LabelWriteError::Persist(PersistError::from(e))
    }
}

#[derive(Debug, Clone)]
struct ResolvedSubject {
    turn_id: i64,
    session_id: i64,
    session_uuid: uuid::Uuid,
    closed: bool,
}

pub(crate) struct Insert<'a> {
    pub(crate) subject_kind: &'a str,
    pub(crate) subject: i64,
    pub(crate) session_id: i64,
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) split: &'a str,
    pub(crate) source: &'a str,
    pub(crate) snapshot: Option<Value>,
    pub(crate) supersede_tier: &'a str,
    pub(crate) rows: Value,
}

pub(crate) async fn insert_rows(
    conn: &mut PgConnection,
    insert: &Insert<'_>,
) -> Result<Vec<EvalLabelRow>, LabelWriteError> {
    let rows = sqlx::query_file_as!(
        EvalLabelRow,
        "src/store/persistence/eval_labels/insert_labels.sql",
        &insert.rows,
        insert.subject_kind,
        insert.subject,
        insert.session_id,
        insert.session_uuid,
        insert.split,
        insert.source,
        insert.snapshot.as_ref(),
        insert.supersede_tier,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

async fn resolve(
    conn: &mut PgConnection,
    subject: &Subject,
) -> Result<ResolvedSubject, LabelWriteError> {
    let found = match subject {
        Subject::Turn { turn_id } => {
            sqlx::query_as!(
                ResolvedSubject,
                r#"SELECT t.id AS "turn_id!", t.session_id AS "session_id!",
                          s.session_uuid AS "session_uuid!", (t.closed_at IS NOT NULL) AS "closed!"
                     FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
                    WHERE t.id = $1"#,
                *turn_id,
            )
            .fetch_optional(&mut *conn)
            .await?
        }
        Subject::SessionEnd { session_uuid } => {
            sqlx::query_as!(
                ResolvedSubject,
                r#"SELECT t.id AS "turn_id!", t.session_id AS "session_id!",
                          s.session_uuid AS "session_uuid!", true AS "closed!"
                     FROM sessions.sessions s
                     JOIN memory.operator_turns t ON t.session_id = s.id AND t.closed_at IS NOT NULL
                    WHERE s.session_uuid = $1
                    ORDER BY t.closed_at DESC, t.id DESC
                    LIMIT 1"#,
                *session_uuid,
            )
            .fetch_optional(&mut *conn)
            .await?
        }
    };
    let found =
        found.ok_or_else(|| LabelWriteError::NotFound("no such turn or session".to_owned()))?;
    if !found.closed {
        return Err(LabelWriteError::Open(
            "This turn is still open. Label it once it has closed.".to_owned(),
        ));
    }
    Ok(found)
}

pub(crate) async fn lock_subject(
    conn: &mut PgConnection,
    kind: &str,
    subject: i64,
) -> Result<(), LabelWriteError> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        format!("eval.labels:{kind}:{subject}"),
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub(crate) async fn assign_turn_split(
    conn: &mut PgConnection,
    turn_id: i64,
    holdout_percent: i64,
) -> Result<String, sqlx::Error> {
    sqlx::query!(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         VALUES ($1, eval.holdout_split($1, $2), 'hash')
         ON CONFLICT (operator_turn_id) DO NOTHING",
        turn_id,
        holdout_percent,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query_scalar!(
        "SELECT split FROM eval.turn_splits WHERE operator_turn_id = $1",
        turn_id,
    )
    .fetch_one(conn)
    .await
}

pub(crate) async fn snapshot(
    conn: &mut PgConnection,
    kind: &str,
    turn_id: i64,
    session_id: i64,
) -> Result<Option<Value>, LabelWriteError> {
    let value = if kind == "session_end" {
        sqlx::query_scalar!(
            r#"SELECT jsonb_build_object(
                      'outcome', o.code, 'judge_model', v.judge_model,
                      'rubric_version', v.rubric_version, 'bundle_id', v.bundle_id,
                      'judged_at', v.judged_at) AS "snapshot!"
                 FROM session_judges.verdicts v JOIN session_judges.outcomes o ON o.id = v.outcome_id
                WHERE v.session_id = $1 AND v.mode = 'live'
                ORDER BY v.judged_at DESC, v.id DESC
                LIMIT 1"#,
            session_id,
        )
        .fetch_optional(conn)
        .await?
    } else {
        sqlx::query_scalar!(
            r#"SELECT jsonb_build_object(
                      'outcome', o.code, 'judge_model', t.judge_model,
                      'rubric_version', t.judge_rubric_version,
                      'bundle_id', t.outcome_bundle_id, 'classified_at', t.classified_at,
                      'rationale', t.rationale) AS "snapshot!"
                 FROM memory.operator_turns t JOIN memory.turn_outcomes o ON o.id = t.outcome_id
                WHERE t.id = $1"#,
            turn_id,
        )
        .fetch_optional(conn)
        .await?
    };
    Ok(value)
}

pub struct GoldWrite<'a> {
    pub subject: Subject,
    pub entries: &'a [GoldEntry],
    pub note: Option<&'a str>,
    pub origin: &'a SpaOrigin,
    pub holdout_percent: i64,
    pub context: &'a str,
    pub eval_run_id: Option<i64>,
}

pub struct NewRelayed<'a> {
    pub subject: Subject,
    pub axis: &'a str,
    pub label: &'a str,
    pub note: Option<&'a str>,
    pub provenance: Value,
    pub holdout_percent: i64,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn write_gold_labels(
        self,
        write: &GoldWrite<'_>,
    ) -> Result<Vec<EvalLabelRow>, LabelWriteError> {
        let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
        let subject = resolve(&mut tx, &write.subject).await?;
        let kind = write.subject.kind();
        lock_subject(&mut tx, kind, subject.turn_id).await?;
        let split = assign_turn_split(&mut tx, subject.turn_id, write.holdout_percent).await?;
        let snap = snapshot(&mut tx, kind, subject.turn_id, subject.session_id).await?;
        let mut provenance = json!({
            "origin": write.origin.origin(),
            "request_id": write.origin.request_id(),
            "context": write.context,
        });
        if let Some(run_id) = write.eval_run_id {
            provenance["eval_run_id"] = json!(run_id);
        }
        let rows: Vec<Value> = write
            .entries
            .iter()
            .map(|e| json!({"axis": e.axis, "label": e.label, "action": "assert", "tier": "gold",
                            "method": "operator_click", "note": write.note, "provenance": provenance}))
            .collect();
        let inserted = insert_rows(
            &mut tx,
            &Insert {
                subject_kind: kind,
                subject: subject.turn_id,
                session_id: subject.session_id,
                session_uuid: subject.session_uuid,
                split: &split,
                source: "spa",
                snapshot: snap,
                supersede_tier: "gold",
                rows: Value::Array(rows),
            },
        )
        .await?;
        tx.commit().await.map_err(PersistError::from)?;
        Ok(inserted)
    }

    pub async fn write_relayed_label(
        self,
        write: &NewRelayed<'_>,
    ) -> Result<EvalLabelRow, LabelWriteError> {
        let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
        let subject = resolve(&mut tx, &write.subject).await?;
        let kind = write.subject.kind();
        lock_subject(&mut tx, kind, subject.turn_id).await?;
        let split = assign_turn_split(&mut tx, subject.turn_id, write.holdout_percent).await?;
        let snap = snapshot(&mut tx, kind, subject.turn_id, subject.session_id).await?;
        let rows = json!([{"axis": write.axis, "label": write.label, "action": "assert",
                           "tier": "relayed", "method": "relayed_quote", "note": write.note,
                           "provenance": write.provenance}]);
        let mut inserted = insert_rows(
            &mut tx,
            &Insert {
                subject_kind: kind,
                subject: subject.turn_id,
                session_id: subject.session_id,
                session_uuid: subject.session_uuid,
                split: &split,
                source: "mcp_relay",
                snapshot: snap,
                supersede_tier: "relayed",
                rows,
            },
        )
        .await?;
        tx.commit().await.map_err(PersistError::from)?;
        inserted
            .pop()
            .ok_or_else(|| LabelWriteError::Invalid("the relayed label was not written".to_owned()))
    }
}
