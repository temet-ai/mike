use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use super::wiring::{Finding, PassOutcome};

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
    let upserted = upsert_all(pool, &outcome.findings).await?;
    let mut resolved = 0;
    for kind in &outcome.checked {
        let subjects: Vec<String> = outcome
            .findings
            .iter()
            .filter(|finding| finding.check_kind == *kind)
            .map(|finding| finding.subject.clone())
            .collect();
        resolved += resolve_missing(pool, kind, &subjects).await?;
    }
    Ok(Tally { upserted, resolved })
}

async fn upsert_all(pool: &PgPool, findings: &[Finding]) -> Result<u64, sqlx::Error> {
    if findings.is_empty() {
        return Ok(0);
    }
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
    let done = sqlx::query!(
        r#"
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
        "#,
        &kinds,
        &subjects,
        &figures,
        &texts
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

async fn resolve_missing(
    pool: &PgPool,
    check_kind: &str,
    still_failing: &[String],
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"
        UPDATE steering.wiring_findings
           SET resolved_at = now()
         WHERE check_kind = $1
           AND resolved_at IS NULL
           AND NOT (subject = ANY($2::text[]))
        "#,
        check_kind,
        still_failing
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
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
