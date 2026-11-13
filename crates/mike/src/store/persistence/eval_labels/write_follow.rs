use serde_json::{Value, json};

use super::super::PersistError;
use super::EvalLabelRow;
use super::write::{Insert, LabelWriteError, insert_rows, lock_subject, snapshot};
use crate::eval::labels::SpaOrigin;

async fn current_row(
    conn: &mut sqlx::PgConnection,
    id: i64,
) -> Result<Option<EvalLabelRow>, LabelWriteError> {
    let row = sqlx::query_file_as!(
        EvalLabelRow,
        "src/store/persistence/eval_labels/label_current_by_id.sql",
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

async fn exists(conn: &mut sqlx::PgConnection, id: i64) -> Result<bool, LabelWriteError> {
    let found = sqlx::query_scalar!("SELECT id FROM eval.labels WHERE id = $1", id)
        .fetch_optional(conn)
        .await?;
    Ok(found.is_some())
}

async fn locked_current(
    conn: &mut sqlx::PgConnection,
    id: i64,
    tiers: &[&str],
) -> Result<EvalLabelRow, LabelWriteError> {
    if !exists(conn, id).await? {
        return Err(LabelWriteError::NotFound(format!("no label {id}")));
    }
    let row = current_row(conn, id)
        .await?
        .ok_or(LabelWriteError::Conflict)?;
    lock_subject(conn, &row.subject_kind, row.subject_id).await?;
    let row = current_row(conn, id)
        .await?
        .ok_or(LabelWriteError::Conflict)?;
    if !tiers.contains(&row.tier.as_str()) {
        return Err(LabelWriteError::Invalid(format!(
            "label {id} is a {} label; only {} labels can be changed here",
            row.tier,
            tiers.join(" or ")
        )));
    }
    Ok(row)
}

fn follow_insert<'a>(
    row: &'a EvalLabelRow,
    source: &'a str,
    snapshot: Option<Value>,
    tier: &'a str,
    rows: Value,
) -> Insert<'a> {
    Insert {
        subject_kind: &row.subject_kind,
        subject: row.subject_id,
        session_id: row.session_id,
        session_uuid: row.session_uuid,
        split: &row.split,
        source,
        snapshot,
        supersede_tier: tier,
        rows,
    }
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn withdraw_label(
        self,
        id: i64,
        origin: &SpaOrigin,
        note: Option<&str>,
    ) -> Result<EvalLabelRow, LabelWriteError> {
        let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
        let row = locked_current(&mut tx, id, &["gold", "relayed"]).await?;
        let rows = json!([{"axis": row.axis, "label": row.label, "action": "withdraw",
                           "tier": row.tier, "method": row.method, "note": note,
                           "provenance": {"origin": origin.origin(), "request_id": origin.request_id()},
                           "supersedes_id": row.id}]);
        let mut inserted = insert_rows(
            &mut tx,
            &follow_insert(&row, "spa", row.verdict_snapshot.clone(), &row.tier, rows),
        )
        .await?;
        tx.commit().await.map_err(PersistError::from)?;
        inserted.pop().ok_or(LabelWriteError::Conflict)
    }

    pub async fn confirm_relayed_label(
        self,
        id: i64,
        origin: &SpaOrigin,
        note: Option<&str>,
    ) -> Result<EvalLabelRow, LabelWriteError> {
        let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
        let row = locked_current(&mut tx, id, &["relayed"]).await?;
        let snap = snapshot(&mut tx, &row.subject_kind, row.subject_id, row.session_id).await?;
        let rows = json!([{"axis": row.axis, "label": row.label, "action": "assert",
                           "tier": "gold", "method": "operator_confirmed_relay", "note": note,
                           "provenance": {"origin": origin.origin(), "request_id": origin.request_id(),
                                          "confirmed_label_id": row.id}}]);
        let mut inserted =
            insert_rows(&mut tx, &follow_insert(&row, "spa", snap, "gold", rows)).await?;
        tx.commit().await.map_err(PersistError::from)?;
        inserted.pop().ok_or(LabelWriteError::Conflict)
    }
}
