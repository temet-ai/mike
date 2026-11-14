use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SUBJECTS: [Option<&str>; 2] = [None, Some("heads up")];

fn answer(pick: &str) -> Value {
    json!({
        "model": "typesafe/jev-1.14",
        "answers": {"wake": {"type": "choice", "choice": pick,
            "probabilities": {"wake": 0.1, "not_yet": 0.8, "unrelated": 0.1}}},
        "usage": {"input_tokens": 10, "output_tokens": 1, "cost": 0.00001}
    })
}

async fn jev(status: u16, body: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/alpha/decisions"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn asked(server: &MockServer) -> usize {
    server.received_requests().await.map_or(0, |r| r.len())
}

fn layer(f: &Fleet, on: bool) {
    let mut toggles = (**f.env.state.feature_toggle_overlay.load()).clone();
    toggles.decisions = on;
    f.env.state.feature_toggle_overlay.store(Arc::new(toggles));
}

async fn claimed(f: &Fleet, to: Uuid, honour_held: bool) -> anyhow::Result<usize> {
    Ok(f.env
        .persist
        .fleet()
        .claim_channel_messages_honouring(to, 10, honour_held)
        .await?
        .len())
}

async fn send_all(f: &Fleet, from: Option<Uuid>, to: Uuid) -> anyhow::Result<()> {
    for subject in SUBJECTS {
        let (status, body) = f.env.send(from, &to.to_string(), subject).await;
        assert_eq!(status, StatusCode::CREATED, "{subject:?}: {body}");
    }
    Ok(())
}

async fn setup(pick: &str, status: u16, on: bool) -> anyhow::Result<Option<(Fleet, MockServer)>> {
    let server = jev(status, answer(pick)).await;
    let Some(f) = fleet_with(Some(&server.uri())).await? else {
        return Ok(None);
    };
    layer(&f, on);
    Ok(Some((f, server)))
}

#[tokio::test]
async fn directed_send_not_yet_answer_holds_the_mail_on_both_paths() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, true).await? else {
        return Ok(());
    };
    send_all(&f, Some(f.t1.session), f.t2.session).await?;
    assert_eq!(asked(&server).await, 2);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 0);
    assert_eq!(claimed(&f, f.t2.session, false).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_wake_answer_delivers_the_mail_on_both_paths() -> anyhow::Result<()> {
    let Some((f, server)) = setup("wake", 200, true).await? else {
        return Ok(());
    };
    send_all(&f, Some(f.t1.session), f.t2.session).await?;
    assert_eq!(asked(&server).await, 2);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_failing_call_delivers_the_mail_on_both_paths() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 500, true).await? else {
        return Ok(());
    };
    send_all(&f, Some(f.t1.session), f.t2.session).await?;
    assert_eq!(asked(&server).await, 2);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_with_the_layer_off_delivers_without_asking() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, false).await? else {
        return Ok(());
    };
    send_all(&f, Some(f.t1.session), f.t2.session).await?;
    assert_eq!(asked(&server).await, 0);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_from_the_operator_is_never_held() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, true).await? else {
        return Ok(());
    };
    send_all(&f, None, f.t2.session).await?;
    assert_eq!(asked(&server).await, 0);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_from_the_coordinator_is_never_held() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, true).await? else {
        return Ok(());
    };
    send_all(&f, Some(f.lead.session), f.t2.session).await?;
    assert_eq!(asked(&server).await, 0);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_send_to_a_non_seat_is_never_held() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, true).await? else {
        return Ok(());
    };
    let outsider_id =
        test_seed::session(f.env.persist.pool(), f.env.project, Some("/tmp/elsewhere")).await?;
    let outsider = f
        .env
        .persist
        .session_identities(&[outsider_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no identity"))?
        .uuid;
    send_all(&f, Some(f.t1.session), outsider).await?;
    assert_eq!(asked(&server).await, 0);
    assert_eq!(claimed(&f, outsider, true).await?, 2);
    f.env.cleanup().await
}

#[tokio::test]
async fn directed_reply_to_the_seats_own_message_is_never_held() -> anyhow::Result<()> {
    let Some((f, server)) = setup("not_yet", 200, true).await? else {
        return Ok(());
    };
    let (status, first) = f
        .env
        .send(Some(f.t2.session), &f.t1.session.to_string(), Some("ask"))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let parent = first["message_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no message_uuid in {first}"))?
        .to_owned();
    let reply = PostSessionMessageBody {
        from_session_id: Some(f.t1.session.to_string()),
        message: "here is the answer".to_owned(),
        thread_id: None,
        in_reply_to: Some(parent),
        subject: None,
        tags: None,
        context: Vec::new(),
    };
    let (status, body) = read(
        post_session_message(
            State(f.env.state.clone()),
            Path(f.t2.session.to_string()),
            Json(reply),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(asked(&server).await, 1);
    assert_eq!(claimed(&f, f.t2.session, true).await?, 1);
    f.env.cleanup().await
}
