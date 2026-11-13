use std::sync::Arc;

use arc_swap::ArcSwap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{
    TestSink, TestVerdict, client_for, client_with_limiter, response_with_usage, test_schema,
};
use super::*;
use crate::llm::budget::RoleSharePcts;
use crate::llm::budget::RoleShares;
use crate::llm::limiter::LimiterLimits;
use crate::settings::SettingsOverlay;

async fn mounted_server(expected: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_with_usage(true, None)))
        .expect(expected)
        .mount(&server)
        .await;
    server
}

async fn call(client: &OpenRouterClient) -> Result<TestVerdict, LlmError> {
    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await
}

#[tokio::test]
async fn every_role_records_the_ids_it_was_given() -> anyhow::Result<()> {
    let server = mounted_server(4).await;
    let cases = [
        (
            LlmRole::Steerer,
            LlmCallContext {
                session_id: Some(7),
                steerer_id: Some(3),
                operator_turn_id: Some(11),
            },
        ),
        (
            LlmRole::SessionJudge,
            LlmCallContext {
                session_id: Some(8),
                steerer_id: None,
                operator_turn_id: None,
            },
        ),
        (
            LlmRole::JudgeWorker,
            LlmCallContext {
                session_id: Some(9),
                steerer_id: None,
                operator_turn_id: Some(12),
            },
        ),
        (LlmRole::LessonRelations, LlmCallContext::default()),
    ];

    for (role, context) in cases {
        let sink = Arc::new(TestSink::default());
        let client = client_for(&server)?
            .with_usage_sink(sink.clone())
            .with_role(role)
            .with_call_context(context);
        call(&client).await?;

        let rows = sink.recorded();
        assert_eq!(rows.len(), 1, "{role:?}");
        assert_eq!(rows[0].role, role);
        assert_eq!(rows[0].session_id, context.session_id, "{role:?}");
        assert_eq!(rows[0].steerer_id, context.steerer_id, "{role:?}");
        assert_eq!(
            rows[0].operator_turn_id, context.operator_turn_id,
            "{role:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn call_context_belongs_to_the_derived_client_not_the_shared_one() -> anyhow::Result<()> {
    let server = mounted_server(2).await;
    let sink = Arc::new(TestSink::default());
    let shared = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::SessionJudge);

    let first = shared.with_call_context(LlmCallContext {
        session_id: Some(101),
        ..LlmCallContext::default()
    });
    let second = shared.with_call_context(LlmCallContext {
        session_id: Some(202),
        ..LlmCallContext::default()
    });
    call(&first).await?;
    call(&second).await?;

    let ids: Vec<Option<i64>> = sink.recorded().iter().map(|r| r.session_id).collect();
    assert_eq!(ids, vec![Some(101), Some(202)]);
    assert_eq!(shared.call_context(), LlmCallContext::default());
    Ok(())
}

#[tokio::test]
async fn refused_call_is_not_recorded_but_carries_no_row_for_any_role() -> anyhow::Result<()> {
    let server = mounted_server(0).await;
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 0, Duration::from_secs(60));
    let sink = Arc::new(TestSink::default());
    let client = client_with_limiter(&server, limiter)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::JudgeWorker)
        .with_call_context(LlmCallContext {
            session_id: Some(1),
            steerer_id: None,
            operator_turn_id: Some(2),
        });

    assert!(call(&client).await.is_err());
    assert!(sink.recorded().is_empty());
    assert_eq!(client.limiter_snapshot().ceiling_refusals_total, 1);
    Ok(())
}

#[tokio::test]
async fn live_overlay_limits_reach_a_client_that_never_had_runtime_settings() -> anyhow::Result<()>
{
    let server = mounted_server(0).await;
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 1000, Duration::from_secs(60));
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay {
        llm_ceiling_max_calls: 0,
        ..SettingsOverlay::default()
    }));
    limiter.install_runtime(overlay, None);
    let client = client_with_limiter(&server, limiter)?.with_role(LlmRole::SessionJudge);

    let result = call(&client).await;
    assert!(
        matches!(result, Err(LlmError::CeilingExceeded { limit: 0, .. })),
        "the live overlay ceiling must govern a role with no runtime settings, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn spend_slice_refusal_makes_no_http_call_and_writes_no_row() -> anyhow::Result<()> {
    let server = mounted_server(0).await;
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        shares: RoleShares::from_pcts(RoleSharePcts {
            steerer: 50,
            session_judge: 50,
            judge_worker: 0,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        }),
        ..LimiterLimits::fixed(
            1000,
            Duration::from_secs(60),
            1000,
            Duration::from_secs(3600),
        )
    });
    let spent = Arc::new(TestSink::with_spend(LlmRole::Steerer, 0.5));
    limiter.set_spend_source(Some(spent));
    let sink = Arc::new(TestSink::default());
    let client = client_with_limiter(&server, limiter)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Steerer);

    let result = call(&client).await;
    assert!(matches!(result, Err(LlmError::SpendCeilingExceeded { .. })));
    assert!(sink.recorded().is_empty());
    assert_eq!(client.limiter_snapshot().spend_refusals_total, 1);
    Ok(())
}

#[tokio::test]
async fn client_budget_and_reservation_follow_the_clients_role() -> anyhow::Result<()> {
    let server = mounted_server(3).await;
    let limiter = CallLimiter::with_limits(LimiterLimits {
        shares: RoleShares::from_pcts(RoleSharePcts {
            steerer: 50,
            session_judge: 50,
            judge_worker: 0,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        }),
        ..LimiterLimits::fixed(10, Duration::from_secs(60), 1000, Duration::from_secs(3600))
    });
    let client = client_with_limiter(&server, limiter)?.with_role(LlmRole::SessionJudge);

    assert_eq!(client.budget().calls_now, 5);
    let reservation = client.reserve(3);
    assert_eq!(reservation.granted(), 3);
    assert_eq!(client.budget().calls_now, 2);

    let reserved = client.with_reservation(&reservation);
    for _ in 0..3 {
        call(&reserved).await?;
    }
    assert_eq!(reservation.remaining(), 0);
    assert_eq!(client.limiter_snapshot().calls_this_minute, 3);
    Ok(())
}
