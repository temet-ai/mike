use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLEAN_REPLY: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hm\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: ping\ndata: {\"type\": \"ping\"}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"All \"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"done.\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"Read\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

async fn upstream_reply(body: &str) -> anyhow::Result<(MockServer, reqwest::Response)> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .mount(&server)
        .await;
    let resp = reqwest::Client::new()
        .get(format!("{}/sse", server.uri()))
        .send()
        .await?;
    Ok((server, resp))
}

async fn ctx(forbidden_words: &[&str], answers_tool_calls: bool) -> anyhow::Result<ForwarderCtx> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = AppState::new(config).await?;
    Ok(ForwarderCtx {
        state,
        upstream_url: String::new(),
        forward_headers: reqwest::header::HeaderMap::new(),
        original_body: Bytes::from_static(b"{}"),
        forbidden_words: forbidden_words.iter().map(|w| (*w).to_owned()).collect(),
        correction_prompt: String::new(),
        answers_tool_calls,
        original_request_id: RequestId::from_raw(1),
        session_id: "s".to_owned(),
        project_path: "/repo".to_owned(),
    })
}

async fn through_the_gate(
    reply: &str,
    forbidden_words: &[&str],
    answers_tool_calls: bool,
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let (_server, upstream) = upstream_reply(reply).await?;
    let (parser_tx, mut parser_rx) = mpsc::channel::<Bytes>(64);
    let (body_tx, mut body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(64);

    let forwarder = tokio::spawn(run_gated_forwarder(
        upstream,
        parser_tx,
        body_tx,
        ctx(forbidden_words, answers_tool_calls).await?,
    ));

    let mut client = Vec::new();
    while let Some(chunk) = body_rx.recv().await {
        client.extend_from_slice(&chunk?);
    }
    let mut parser = Vec::new();
    while let Some(chunk) = parser_rx.recv().await {
        parser.extend_from_slice(&chunk);
    }
    forwarder.await?;
    Ok((client, parser))
}

#[tokio::test]
async fn a_reply_with_nothing_to_act_on_reaches_the_client_byte_for_byte() -> anyhow::Result<()> {
    let (client, parser) = through_the_gate(CLEAN_REPLY, &["pre-existing"], true).await?;
    assert_eq!(
        String::from_utf8(client)?,
        CLEAN_REPLY,
        "the gate may delay text by a few characters, never change or drop it"
    );
    assert_eq!(
        String::from_utf8(parser)?,
        CLEAN_REPLY,
        "capture reads the same bytes the upstream sent"
    );
    Ok(())
}

const CUT_REPLY: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"The database migration fail\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ed because of a pre-\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"existing table.\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

const CONTINUATION: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\"}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" table that already\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" existed.\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

async fn correction_upstream(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(CONTINUATION),
        )
        .mount(server)
        .await;
}

fn text_of(client: &[u8]) -> anyhow::Result<String> {
    let mut text = String::new();
    for frame in SseFramer::default().push(client) {
        let json: serde_json::Value = serde_json::from_str(&frame.data)?;
        if json
            .pointer("/delta/type")
            .and_then(serde_json::Value::as_str)
            == Some("text_delta")
        {
            text.push_str(json["delta"]["text"].as_str().unwrap_or_default());
        }
    }
    Ok(text)
}

async fn assistant_turn_sent(server: &MockServer) -> anyhow::Result<serde_json::Value> {
    let requests = server.received_requests().await.unwrap_or_default();
    let Some(correction) = requests.iter().find(|r| r.method.as_str() == "POST") else {
        anyhow::bail!("no correction request reached upstream");
    };
    let body: serde_json::Value = serde_json::from_slice(&correction.body)?;
    Ok(body["messages"][1].clone())
}

#[tokio::test]
async fn word_cut_with_a_continuation_client_reads_one_reply_without_the_word() -> anyhow::Result<()>
{
    let (server, upstream) = upstream_reply(CUT_REPLY).await?;
    correction_upstream(&server).await;
    let mut ctx = ctx(&["pre-existing"], false).await?;
    ctx.upstream_url = format!("{}/v1/messages", server.uri());
    ctx.original_body =
        Bytes::from_static(br#"{"messages":[{"role":"user","content":"Why did it fail?"}]}"#);
    ctx.correction_prompt =
        crate::features_settings::sections::weasel_intercept::DEFAULT_CORRECTION_PROMPT.to_owned();

    let (parser_tx, mut parser_rx) = mpsc::channel::<Bytes>(64);
    let (body_tx, mut body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    let forwarder = tokio::spawn(run_gated_forwarder(upstream, parser_tx, body_tx, ctx));
    let drain = tokio::spawn(async move { while parser_rx.recv().await.is_some() {} });

    let mut client = Vec::new();
    while let Some(chunk) = body_rx.recv().await {
        client.extend_from_slice(&chunk?);
    }
    forwarder.await?;
    drain.await?;

    let text = text_of(&client)?;
    assert_eq!(
        text,
        "The database migration failed because of a table that already existed."
    );
    assert!(!String::from_utf8(client)?.contains("pre-"));

    let assistant = assistant_turn_sent(&server).await?;
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(
        assistant["content"][0]["text"],
        "The database migration failed because of a"
    );
    Ok(())
}

#[tokio::test]
async fn bytes_after_the_last_blank_line_are_not_lost() -> anyhow::Result<()> {
    let reply = "event: ping\ndata: {\"type\": \"ping\"}\n\nevent: message_stop\ndata: {}";
    let (client, _parser) = through_the_gate(reply, &["pre-existing"], true).await?;
    assert_eq!(String::from_utf8(client)?, reply);
    Ok(())
}
