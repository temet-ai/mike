use crate::store::persistence::runtime_repo::RuntimePersist as _;
use uuid::Uuid;

use super::ObservationLimits;
use super::tests_observation_support::{fixture, observation};
use crate::runtime::{ModelCallObservation, UsageSnapshot};

#[tokio::test]
async fn observation_identity_refusal_rolls_back_new_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let incoming = Uuid::new_v4();
    let limits = ObservationLimits {
        max_bytes: 1024,
        ..Default::default()
    };
    assert!(
        f.handle
            .ensure_runtime_session_with_limits(incoming, &limits)
            .await
            .is_err()
    );
    assert!(
        sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
            incoming
        )
        .fetch_optional(f.handle.pool())
        .await?
        .is_none()
    );
    let id = f.handle.ensure_runtime_session(incoming).await?;
    assert_eq!(f.handle.ensure_runtime_session(incoming).await?, id);
    let charged = sqlx::query_scalar!(
        "SELECT retained_bytes FROM runtime.session_identities WHERE session_id = $1",
        id
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(charged, 65_536);
    f.cleanup().await
}

#[tokio::test]
async fn observation_usage_prefers_wire_without_summing_overlapping_otel_or_unknown_scope()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.source.kind = "otel".into();
    o.model_call = Some(ModelCallObservation {
        call_id: "otel-one".into(),
        status: Some("completed".into()),
        duration_ms: None,
        http_status: None,
        usage: Some(UsageSnapshot {
            scope: "own".into(),
            basis: "call".into(),
            input_tokens: Some(100),
            output_tokens: Some(20),
            ..Default::default()
        }),
    });
    f.handle.runtime().apply_session_observation(&o).await?;
    assert_eq!(
        f.handle
            .runtime()
            .session_observations(&[f.session])
            .await?[0]
            .input_tokens,
        Some(100)
    );
    o.observation_id = Uuid::new_v4();
    o.source.kind = "responses_wire".into();
    let call = o
        .model_call
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("call missing"))?;
    call.call_id = "wire-one".into();
    call.usage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("usage missing"))?
        .input_tokens = Some(120);
    f.handle.runtime().apply_session_observation(&o).await?;
    let row = &f
        .handle
        .runtime()
        .session_observations(&[f.session])
        .await?[0];
    assert_eq!(row.input_tokens, Some(120));
    assert_eq!(row.model_call_count, 1);
    o.observation_id = Uuid::new_v4();
    let call = o
        .model_call
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("call missing"))?;
    call.call_id = "unknown-scope".into();
    call.usage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("usage missing"))?
        .scope = "unknown".into();
    f.handle.runtime().apply_session_observation(&o).await?;
    assert_eq!(
        f.handle
            .runtime()
            .session_observations(&[f.session])
            .await?[0]
            .input_tokens,
        Some(120)
    );
    o.observation_id = Uuid::new_v4();
    o.usage = Some(UsageSnapshot {
        scope: "thread".into(),
        basis: "provider_cumulative".into(),
        input_tokens: Some(500),
        ..Default::default()
    });
    f.handle.runtime().apply_session_observation(&o).await?;
    assert_eq!(
        f.handle
            .runtime()
            .session_observations(&[f.session])
            .await?[0]
            .input_tokens,
        Some(500)
    );
    f.cleanup().await
}
