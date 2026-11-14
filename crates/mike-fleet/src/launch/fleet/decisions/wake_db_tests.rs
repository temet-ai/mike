use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::test_support::DecisionsFixture;
use super::*;
use crate::feature_toggles::FeatureToggles;
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;
use crate::store::persistence::mailbox::ThreadedMessageInsert;

fn choice_body(pick: &str) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {"wake": {"type": "choice", "choice": pick,
            "probabilities": {"wake": 0.1, "not_yet": 0.8, "unrelated": 0.1}}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn stub(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

struct Layer {
    decisions: FleetDecisions,
    toggles: Arc<ArcSwap<FeatureToggles>>,
}

fn layer(server: &MockServer, fx: &DecisionsFixture, on: bool) -> anyhow::Result<Layer> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    let toggles = Arc::new(ArcSwap::from_pointee(FeatureToggles {
        decisions: on,
        ..FeatureToggles::default()
    }));
    let decisions = FleetDecisions::new(
        client,
        fx.handle.clone(),
        toggles.clone(),
        Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())),
    );
    Ok(Layer { decisions, toggles })
}

fn mail<'a>(fx: &DecisionsFixture, from: Option<Uuid>, body: &'a str) -> Mail<'a> {
    Mail {
        to: fx.worker,
        from,
        in_reply_to_id: None,
        body,
        sender_label: Some("worker-T2"),
        message_uuid: None,
    }
}

async fn queue(fx: &DecisionsFixture, from: Uuid, held: bool) -> anyhow::Result<Uuid> {
    let to = fx.handle.launch().resolve_session_core(fx.worker).await?;
    let from_id = fx.handle.launch().resolve_session_core(from).await?;
    let uuid = Uuid::new_v4();
    fx.handle
        .launch()
        .insert_threaded_message_with_context(
            ThreadedMessageInsert {
                message_uuid: uuid,
                to_session_id: to.map(|t| t.0),
                from_session_id: from_id.map(|t| t.0),
                body: "a broadcast",
                thread_id: None,
                in_reply_to_id: None,
                held,
            },
            &[],
        )
        .await?;
    Ok(uuid)
}

#[tokio::test]
async fn hold_a_not_yet_answer_holds_and_the_claim_skips_it_while_the_check_is_on()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, choice_body("not_yet")).await;
    let on = layer(&server, &fx, true)?;
    assert!(
        hold_with(
            &fx.handle,
            Some(&on.decisions),
            &mail(&fx, Some(fx.peer), "fyi")
        )
        .await
    );
    queue(&fx, fx.peer, true).await?;
    assert!(
        fx.handle
            .fleet()
            .claim_channel_messages_honouring(fx.worker, 10, true)
            .await?
            .is_empty()
    );
    assert_eq!(
        fx.handle
            .launch()
            .fetch_mail_for_drain(fx.worker, 10)
            .await?
            .len(),
        1
    );
    fx.finish().await
}

#[tokio::test]
async fn flipping_the_toggle_off_releases_held_mail_and_the_next_decision_is_not_held()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, choice_body("unrelated")).await;
    let on = layer(&server, &fx, true)?;
    let held = queue(&fx, fx.peer, true).await?;
    assert!(honouring_with(&fx.handle, on.decisions.layer_on(), fx.worker).await);
    on.toggles.store(Arc::new(FeatureToggles {
        decisions: false,
        ..FeatureToggles::default()
    }));
    assert!(!honouring_with(&fx.handle, on.decisions.layer_on(), fx.worker).await);
    assert!(
        !hold_with(
            &fx.handle,
            Some(&on.decisions),
            &mail(&fx, Some(fx.peer), "fyi")
        )
        .await
    );
    let claimed = fx
        .handle
        .fleet()
        .claim_channel_messages_honouring(fx.worker, 10, false)
        .await?;
    assert_eq!(
        claimed.iter().map(|m| m.message_uuid).collect::<Vec<_>>(),
        vec![held]
    );
    fx.finish().await
}

#[tokio::test]
async fn flipping_the_toggle_on_makes_the_next_decision_hold_with_no_restart() -> anyhow::Result<()>
{
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, choice_body("not_yet")).await;
    let off = layer(&server, &fx, false)?;
    assert!(
        !hold_with(
            &fx.handle,
            Some(&off.decisions),
            &mail(&fx, Some(fx.peer), "fyi")
        )
        .await
    );
    off.toggles.store(Arc::new(FeatureToggles {
        decisions: true,
        ..FeatureToggles::default()
    }));
    assert!(
        hold_with(
            &fx.handle,
            Some(&off.decisions),
            &mail(&fx, Some(fx.peer), "fyi")
        )
        .await
    );
    fx.finish().await
}

#[tokio::test]
async fn hold_a_failing_client_takes_the_off_path_and_wakes_the_seat() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    for server in [
        stub(500, json!({})).await,
        stub(200, json!("not an answer")).await,
    ] {
        let on = layer(&server, &fx, true)?;
        assert!(
            !hold_with(
                &fx.handle,
                Some(&on.decisions),
                &mail(&fx, Some(fx.peer), "fyi")
            )
            .await
        );
    }
    fx.finish().await
}

#[tokio::test]
async fn hold_operator_mail_and_the_coordinators_mail_are_never_held_and_never_asked()
-> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, choice_body("unrelated")).await;
    let on = layer(&server, &fx, true)?;
    assert!(
        !hold_with(
            &fx.handle,
            Some(&on.decisions),
            &mail(&fx, None, "operator")
        )
        .await
    );
    assert!(
        !hold_with(
            &fx.handle,
            Some(&on.decisions),
            &mail(&fx, Some(fx.coordinator), "plan")
        )
        .await
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    fx.finish().await
}

#[tokio::test]
async fn hold_a_reply_to_the_seats_own_message_is_never_held() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({})).await? else {
        return Ok(());
    };
    let server = stub(200, choice_body("unrelated")).await;
    let on = layer(&server, &fx, true)?;
    let own = queue(&fx, fx.worker, false).await?;
    let own_id = fx.handle.launch().fetch_message_core_by_uuid(own).await?;
    let mut reply = mail(&fx, Some(fx.peer), "an answer");
    reply.in_reply_to_id = own_id.map(|core| core.id);
    assert!(!hold_with(&fx.handle, Some(&on.decisions), &reply).await);
    fx.finish().await
}

#[tokio::test]
async fn hold_a_point_set_off_in_the_definition_never_holds() -> anyhow::Result<()> {
    let Some(fx) = DecisionsFixture::boot(json!({"decisions": {"wake_check": "off"}})).await?
    else {
        return Ok(());
    };
    let server = stub(200, choice_body("unrelated")).await;
    let on = layer(&server, &fx, true)?;
    assert!(
        !hold_with(
            &fx.handle,
            Some(&on.decisions),
            &mail(&fx, Some(fx.peer), "fyi")
        )
        .await
    );
    assert!(!honouring_with(&fx.handle, true, fx.worker).await);
    fx.finish().await
}
