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

#[tokio::test]
async fn bytes_after_the_last_blank_line_are_not_lost() -> anyhow::Result<()> {
    let reply = "event: ping\ndata: {\"type\": \"ping\"}\n\nevent: message_stop\ndata: {}";
    let (client, _parser) = through_the_gate(reply, &["pre-existing"], true).await?;
    assert_eq!(String::from_utf8(client)?, reply);
    Ok(())
}
