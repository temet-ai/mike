use reqwest::StatusCode;
use uuid::Uuid;

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::seed_project;

const COUNT_FIELDS: [&str; 7] = [
    "files_scanned",
    "found",
    "told",
    "fixed_after_told",
    "fixed",
    "ignored",
    "open",
];

#[tokio::test]
async fn overview_reports_code_finding_counts_for_a_session_with_no_scans() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let pool = harness.handle.pool();
    let project_id = seed_project(pool, &format!("/overview-code/{}", Uuid::new_v4())).await?;
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project_id)
    .execute(pool)
    .await?;

    let (status, overview) = harness
        .get(&format!("/api/sessions/{session}/overview"))
        .await?;

    assert_eq!(status, StatusCode::OK, "{overview}");
    let counts = &overview["code_findings"];
    assert!(counts.is_object(), "{overview}");
    for field in COUNT_FIELDS {
        assert_eq!(counts[field], 0, "{field}: {counts}");
    }
    harness.cleanup().await
}
