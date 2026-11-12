use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::diagnostics::{DiagnosticExchange, DiagnosticLimits, EvidenceStatus};
use super::tests_observation_support::{fixture, observation};

fn exchange(session: Option<Uuid>) -> DiagnosticExchange {
    DiagnosticExchange {
        session_id: session,
        provider_session_id: None,
        provider_thread_id: None,
        provider_turn_id: None,
        request_kind: None,
        correlation: json!({}),
        auth_route: "api_key".into(),
        transport: "http".into(),
        method: "POST".into(),
        path: "/responses".into(),
        upstream_url: "https://example.invalid/responses".into(),
        request_headers: json!({}),
        forwarded_headers: json!({}),
        request_body: Some(vec![b'q'; 4096]),
        forwarded_body: Some(vec![b'q'; 4096]),
        request_body_encoding: None,
        model: Some("fixture".into()),
        response_status: Some(200),
        response_headers: None,
        response_body: Some(vec![b'r'; 4096]),
        upstream_request_id: None,
        response_id: None,
        response_model: None,
        response_state: None,
        usage: None,
        stream_events: None,
        error: None,
        status: "complete".into(),
        created_at: Utc::now(),
        evidence_status: EvidenceStatus::Retained,
    }
}

#[tokio::test]
async fn diagnostic_default_off_preserves_facts_without_payload_copies() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let o = observation(f.session)?;
    f.handle.apply_session_observation(&o).await?;
    let result = f
        .handle
        .record_responses_diagnostic(&exchange(Some(f.session)), &DiagnosticLimits::default())
        .await?;
    assert_eq!(result.evidence_status, EvidenceStatus::NotCollected);
    let id = result
        .exchange_id
        .ok_or_else(|| anyhow::anyhow!("metadata missing"))?;
    let row = sqlx::query!(
        "SELECT request_body, forwarded_body, response_body, output_items
        FROM runtime.responses_exchanges WHERE id = $1",
        id
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert!(
        row.request_body.is_none() && row.forwarded_body.is_none() && row.response_body.is_none()
    );
    assert_eq!(row.output_items, json!([]));
    assert_eq!(
        f.handle.session_observations(&[f.session]).await?[0]
            .observed_model
            .as_deref(),
        Some("actual-model")
    );
    f.cleanup().await
}

#[tokio::test]
async fn diagnostic_concurrent_byte_quota_refuses_wire_and_expiry_reclaims_unlinked()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let e = exchange(None);
    let limits = DiagnosticLimits {
        enabled: true,
        max_total_bytes: 13_000,
        ..Default::default()
    };
    let (a, b) = tokio::join!(
        f.handle.record_responses_diagnostic(&e, &limits),
        f.handle.record_responses_diagnostic(&e, &limits)
    );
    let statuses = [a?.evidence_status, b?.evidence_status];
    assert!(statuses.contains(&EvidenceStatus::Retained));
    assert!(statuses.contains(&EvidenceStatus::BudgetRefused));
    let used = sqlx::query_scalar!(
        "SELECT retained_bytes FROM runtime.storage_accounting WHERE kind = 'diagnostics'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert!(used <= limits.max_total_bytes);
    let bodies = sqlx::query!(
        "SELECT forwarded_body, request_body, response_body FROM runtime.responses_exchanges
        WHERE evidence_status = 'retained'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        bodies.request_body.as_deref(),
        Some(vec![b'q'; 4096].as_slice())
    );
    assert!(bodies.forwarded_body.is_none());
    assert!(bodies.response_body.is_some());
    let swept = f
        .handle
        .sweep_runtime_observations(Utc::now() + Duration::days(2), 100)
        .await?;
    assert_eq!(swept.diagnostics_expired, 1);
    let expired = sqlx::query!(
        "SELECT request_body, response_body FROM runtime.responses_exchanges
        WHERE evidence_status = 'expired'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert!(expired.request_body.is_none() && expired.response_body.is_none());
    let swept = f
        .handle
        .sweep_runtime_observations(Utc::now() + Duration::days(8), 100)
        .await?;
    assert_eq!(swept.diagnostics_removed, 2);
    let bytes = sqlx::query_scalar!(
        "SELECT retained_bytes FROM runtime.storage_accounting WHERE kind = 'diagnostics'"
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(bytes, 0);
    f.cleanup().await
}

#[tokio::test]
async fn diagnostic_record_and_session_ceilings_leave_honest_availability() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let e = exchange(Some(f.session));
    let mut limits = DiagnosticLimits {
        enabled: true,
        max_record_bytes: 100,
        ..Default::default()
    };
    let result = f.handle.record_responses_diagnostic(&e, &limits).await?;
    assert_eq!(result.evidence_status, EvidenceStatus::BudgetRefused);
    assert!(result.exchange_id.is_some());
    limits.max_session_bytes = 1;
    let result = f.handle.record_responses_diagnostic(&e, &limits).await?;
    assert_eq!(result.evidence_status, EvidenceStatus::BudgetRefused);
    assert!(result.exchange_id.is_none());
    sqlx::query!("DELETE FROM sessions.sessions WHERE id = $1", f.session_db)
        .execute(f.handle.pool())
        .await?;
    let used = sqlx::query!("SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'diagnostics'")
        .fetch_one(f.handle.pool()).await?;
    assert_eq!((used.retained_rows, used.retained_bytes), (0, 0));
    f.cleanup().await
}
