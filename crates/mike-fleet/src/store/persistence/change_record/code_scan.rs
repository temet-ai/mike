use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone)]
pub struct PendingChange {
    pub id: i64,
    pub path: String,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NewFinding {
    pub rule_id: String,
    pub rule_source: String,
    pub severity: String,
    pub message: String,
    pub line: i32,
    pub col: i32,
    pub matched: String,
    pub fix: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewCodeScan {
    pub change_event_id: i64,
    pub checkout: String,
    pub path: String,
    pub language: Option<String>,
    pub rules_version: String,
    pub rules_run: i32,
    pub bytes: i32,
    pub duration_us: i32,
    pub skipped: Option<String>,
    pub error: Option<String>,
    pub findings_before: i32,
    pub findings_after: i32,
    pub present: Vec<String>,
    pub added: Vec<NewFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedScan {
    pub scan_id: i64,
    pub fixed: i64,
}

#[derive(Debug, Clone)]
pub struct ReadyFinding {
    pub id: i64,
    pub session: Uuid,
    pub path: String,
    pub line: i32,
    pub rule_id: String,
    pub message: String,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeFindingRow {
    pub id: i64,
    pub scan_id: i64,
    pub change_event_id: i64,
    pub session_id: Option<Uuid>,
    pub checkout: String,
    pub path: String,
    pub rule_id: String,
    pub rule_source: String,
    pub severity: String,
    pub message: String,
    pub line: i32,
    pub col: i32,
    pub matched: String,
    pub fix: Option<String>,
    pub found_at: DateTime<Utc>,
    pub delivered_request_id: Option<i64>,
    pub delivered_via: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub outcome: String,
    pub outcome_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeScanSummary {
    pub scans: i64,
    pub scanned: i64,
    pub skipped: i64,
    pub failed: i64,
    pub findings_new: i64,
    pub findings_fixed: i64,
    pub median_us: i64,
    pub max_us: i64,
    pub last_scan_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleCount {
    pub rule_id: String,
    pub source: String,
    pub found: i64,
    pub delivered: i64,
    pub pushed: i64,
    pub fixed: i64,
    pub fixed_after_told: i64,
    pub fixed_untold: i64,
    pub fixed_by_other: i64,
    pub ignored: i64,
    pub open: i64,
    pub median_lag_s: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SessionCodeCounts {
    pub files_scanned: i64,
    pub found: i64,
    pub told: i64,
    pub fixed_after_told: i64,
    pub fixed: i64,
    pub fixed_by_other: i64,
    pub ignored: i64,
    pub open: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Settled {
    pub fixed_by_other: i64,
    pub ignored: i64,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn code_scan_cursor(self, checkout: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("code_scan_cursor", async move {
            let cursor = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/code_scan/cursor.sql",
                checkout,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(cursor)
        })
        .await
    }

    pub async fn code_scan_pending(
        self,
        checkout: &str,
        after: i64,
        limit: i64,
    ) -> Result<Vec<PendingChange>, PersistError> {
        crate::metrics::time_db("code_scan_pending", async move {
            let rows = sqlx::query_file_as!(
                PendingChange,
                "src/store/persistence/change_record/code_scan/pending.sql",
                checkout,
                after,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn record_code_scan(
        self,
        scan: &NewCodeScan,
    ) -> Result<Option<RecordedScan>, PersistError> {
        crate::metrics::time_db("code_scan_record", async move {
            let mut tx = self.pool.begin().await?;
            let added = i32::try_from(scan.added.len()).unwrap_or(i32::MAX);
            let Some(scan_id) = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/code_scan/insert_scan.sql",
                scan.change_event_id,
                scan.checkout,
                scan.path,
                scan.language,
                scan.rules_version,
                scan.rules_run,
                scan.bytes,
                scan.duration_us,
                scan.skipped,
                scan.error,
                scan.findings_before,
                scan.findings_after,
                added,
            )
            .fetch_optional(&mut *tx)
            .await?
            else {
                return Ok(None);
            };
            let read_whole_file = scan.skipped.is_none() && scan.error.is_none();
            let fixed = if read_whole_file {
                close_fixed(&mut tx, scan, scan_id).await?
            } else {
                0
            };
            insert_findings(&mut tx, scan, scan_id).await?;
            tx.commit().await?;
            Ok(Some(RecordedScan { scan_id, fixed }))
        })
        .await
    }

    pub async fn supersede_deleted_paths(
        self,
        checkout: &str,
        paths: &[String],
    ) -> Result<u64, PersistError> {
        if paths.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("code_findings_supersede_deleted", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/change_record/code_scan/supersede_deleted.sql",
                checkout,
                paths,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn settle_code_findings(self) -> Result<Settled, PersistError> {
        crate::metrics::time_db("code_findings_settle", async move {
            let row = sqlx::query_file!("src/store/persistence/change_record/code_scan/settle.sql")
                .fetch_one(&self.pool)
                .await?;
            Ok(Settled {
                fixed_by_other: row.fixed_by_other,
                ignored: row.ignored,
            })
        })
        .await
    }

    pub async fn session_code_counts(
        self,
        session: Uuid,
    ) -> Result<SessionCodeCounts, PersistError> {
        crate::metrics::time_db("code_findings_session_counts", async move {
            let counts = sqlx::query_file_as!(
                SessionCodeCounts,
                "src/store/persistence/change_record/code_scan/session_counts.sql",
                session,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(counts)
        })
        .await
    }

    pub async fn code_findings_ready_to_post(
        self,
        checkouts: &[String],
        after: i64,
        severities: &[String],
    ) -> Result<Vec<ReadyFinding>, PersistError> {
        crate::metrics::time_db("code_findings_ready_to_post", async move {
            let rows = sqlx::query_file_as!(
                ReadyFinding,
                "src/store/persistence/change_record/code_scan/ready_to_post.sql",
                checkouts,
                after,
                severities,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn code_findings_unnamed_floor(
        self,
        checkouts: &[String],
        after: i64,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("code_findings_unnamed_floor", async move {
            let floor = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/code_scan/unnamed_floor.sql",
                checkouts,
                after,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(floor)
        })
        .await
    }

    pub async fn mark_code_findings_delivered(
        self,
        delivered: &[mike_base::code_notes::Delivered],
    ) -> Result<u64, PersistError> {
        let findings: Vec<i64> = delivered.iter().map(|d| d.finding_id).collect();
        let requests: Vec<Option<i64>> = delivered.iter().map(|d| d.request_id).collect();
        let via: Vec<String> = delivered.iter().map(|d| d.via.to_string()).collect();
        crate::metrics::time_db("code_findings_mark_delivered", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/change_record/code_scan/mark_delivered.sql",
                &findings,
                &requests as &[Option<i64>],
                &via,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn list_code_findings(
        self,
        since: DateTime<Utc>,
        session: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<CodeFindingRow>, PersistError> {
        crate::metrics::time_db("code_findings_list", async move {
            let rows = sqlx::query_file_as!(
                CodeFindingRow,
                "src/store/persistence/change_record/code_scan/list.sql",
                since,
                session,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn code_scan_summary(
        self,
        since: DateTime<Utc>,
    ) -> Result<(CodeScanSummary, Vec<RuleCount>), PersistError> {
        crate::metrics::time_db("code_scan_summary", async move {
            let summary = sqlx::query_file_as!(
                CodeScanSummary,
                "src/store/persistence/change_record/code_scan/summary.sql",
                since,
            )
            .fetch_one(&self.pool)
            .await?;
            let rules = sqlx::query_file_as!(
                RuleCount,
                "src/store/persistence/change_record/code_scan/rule_counts.sql",
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok((summary, rules))
        })
        .await
    }
}

async fn close_fixed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    scan: &NewCodeScan,
    scan_id: i64,
) -> Result<i64, PersistError> {
    let fixed = sqlx::query_file_scalar!(
        "src/store/persistence/change_record/code_scan/close_fixed.sql",
        scan.checkout,
        scan.path,
        scan_id,
        &scan.present,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(i64::from(fixed))
}

async fn insert_findings(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    scan: &NewCodeScan,
    scan_id: i64,
) -> Result<(), PersistError> {
    if scan.added.is_empty() {
        return Ok(());
    }
    let column = |pick: fn(&NewFinding) -> String| scan.added.iter().map(pick).collect::<Vec<_>>();
    let rule_ids = column(|f| f.rule_id.clone());
    let sources = column(|f| f.rule_source.clone());
    let severities = column(|f| f.severity.clone());
    let messages = column(|f| f.message.clone());
    let matched = column(|f| f.matched.clone());
    let fingerprints = column(|f| f.fingerprint.clone());
    let fixes: Vec<Option<String>> = scan.added.iter().map(|f| f.fix.clone()).collect();
    let lines: Vec<i32> = scan.added.iter().map(|f| f.line).collect();
    let cols: Vec<i32> = scan.added.iter().map(|f| f.col).collect();
    sqlx::query_file!(
        "src/store/persistence/change_record/code_scan/insert_findings.sql",
        scan_id,
        scan.change_event_id,
        scan.checkout,
        scan.path,
        &rule_ids,
        &sources,
        &severities,
        &messages,
        &lines,
        &cols,
        &matched,
        &fixes as &[Option<String>],
        &fingerprints,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
