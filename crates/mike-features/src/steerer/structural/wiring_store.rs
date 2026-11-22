use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use super::wiring::PassOutcome;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub upserted: u64,
    pub resolved: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredFinding {
    pub id: i64,
    pub check_kind: String,
    pub subject: String,
    pub figure_value: f64,
    pub figure_text: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub figure_changed_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub async fn apply(pool: &PgPool, outcome: &PassOutcome) -> Result<Tally, sqlx::Error> {
    if outcome.checked.is_empty() {
        return Ok(Tally::default());
    }
    let findings = &outcome.findings;
    let checked: Vec<String> = outcome
        .checked
        .iter()
        .map(|kind| (*kind).to_owned())
        .collect();
    let kinds: Vec<String> = findings
        .iter()
        .map(|finding| finding.check_kind.to_owned())
        .collect();
    let subjects: Vec<String> = findings
        .iter()
        .map(|finding| finding.subject.clone())
        .collect();
    let figures: Vec<f64> = findings
        .iter()
        .map(|finding| finding.figure_value)
        .collect();
    let texts: Vec<String> = findings
        .iter()
        .map(|finding| finding.figure_text.clone())
        .collect();
    let row = sqlx::query!(
        r#"
        WITH upserted AS (
            INSERT INTO steering.wiring_findings (check_kind, subject, figure_value, figure_text)
            SELECT * FROM UNNEST($1::text[], $2::text[], $3::float8[], $4::text[])
            ON CONFLICT (check_kind, subject) DO UPDATE
               SET first_seen = CASE WHEN steering.wiring_findings.resolved_at IS NULL
                                     THEN steering.wiring_findings.first_seen ELSE now() END,
                   figure_changed_at = CASE WHEN steering.wiring_findings.resolved_at IS NULL
                                             AND steering.wiring_findings.figure_value = EXCLUDED.figure_value
                                            THEN steering.wiring_findings.figure_changed_at ELSE now() END,
                   figure_value = EXCLUDED.figure_value,
                   figure_text = EXCLUDED.figure_text,
                   last_seen = now(),
                   resolved_at = NULL
            RETURNING 1
        ),
        resolved AS (
            UPDATE steering.wiring_findings w
               SET resolved_at = now()
             WHERE w.resolved_at IS NULL
               AND w.check_kind = ANY($5::text[])
               AND NOT EXISTS (
                   SELECT 1 FROM UNNEST($1::text[], $2::text[]) AS f(check_kind, subject)
                    WHERE f.check_kind = w.check_kind AND f.subject = w.subject)
            RETURNING 1
        )
        SELECT (SELECT count(*) FROM upserted) AS "upserted!",
               (SELECT count(*) FROM resolved) AS "resolved!"
        "#,
        &kinds,
        &subjects,
        &figures,
        &texts,
        &checked
    )
    .fetch_one(pool)
    .await?;
    Ok(Tally {
        upserted: u64::try_from(row.upserted).unwrap_or_default(),
        resolved: u64::try_from(row.resolved).unwrap_or_default(),
    })
}

pub async fn list_findings(
    pool: &PgPool,
    resolved_since: DateTime<Utc>,
) -> Result<Vec<StoredFinding>, sqlx::Error> {
    sqlx::query_as!(
        StoredFinding,
        r#"
        SELECT id, check_kind, subject, figure_value, figure_text,
               first_seen, last_seen, figure_changed_at, resolved_at
          FROM steering.wiring_findings
         WHERE resolved_at IS NULL OR resolved_at > $1
         ORDER BY (resolved_at IS NOT NULL), first_seen, check_kind, subject
        "#,
        resolved_since
    )
    .fetch_all(pool)
    .await
}
