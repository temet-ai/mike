use crate::anthropic_fixtures;

use super::harness::{
    lazy_mock_upstream, start_proxy_with_config, start_proxy_with_config_upstream,
};
use super::lazy_section_support::{
    CHROME_HEADING, CHROME_STUB, CHROME_TOKEN, create_lazy_chrome_rule, lazy_request_body,
};

use anthropic_fixtures::{RequestBody, text_block};
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn lazy_section_rule_crud_round_trips_trigger_token() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "lazy_section",
            "value": CHROME_HEADING,
            "action": "replace",
            "text": CHROME_STUB,
            "trigger_token": CHROME_TOKEN,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    let dto = &envelope["rule"];
    assert_eq!(dto["kind"], "lazy_section");
    assert_eq!(dto["trigger_token"], CHROME_TOKEN);
    assert_eq!(dto["text"], CHROME_STUB);
    let id = dto["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("rule id must be a string"))?
        .to_string();

    let rules: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let listed = rules
        .iter()
        .find(|r| r["id"] == json!(id))
        .ok_or_else(|| anyhow::anyhow!("created rule missing from list"))?;
    assert_eq!(listed["trigger_token"], CHROME_TOKEN);

    let bad = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "lazy_section",
            "value": "# Other section",
            "action": "replace",
            "text": "stub",
        }))
        .send()
        .await?;
    assert_eq!(bad.status(), 400);
    let err: Value = bad.json().await?;
    assert!(
        err["error"]
            .as_str()
            .unwrap_or("")
            .contains("`trigger_token`"),
        "validator message must surface, got: {err}"
    );

    let patched = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({
            "kind": "xml_tag",
            "value": "system-reminder",
            "action": "drop",
            "text": null,
            "trigger_token": null,
        }))
        .send()
        .await?;
    assert_eq!(patched.status(), 200);
    let patched: Value = patched.json().await?;
    assert_eq!(patched["rule"]["kind"], "xml_tag");
    assert!(
        patched["rule"].get("trigger_token").is_none(),
        "cleared trigger_token must be absent from the DTO"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn lazy_section_request_1_stripped_original_preserved() -> anyhow::Result<()> {
    let mock_server = lazy_mock_upstream().await;
    let Some(proxy) =
        start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri()).await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    create_lazy_chrome_rule(&client, &proxy.inspect_url, true).await?;

    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", uuid::Uuid::new_v4().to_string())
        .json(&lazy_request_body(json!([
            {"role": "user", "content": [{"type": "text", "text": "hello"}]}
        ])))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    let upstream: Value = serde_json::from_slice(&received[0].body)?;
    let sys_text = upstream["system"][1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("system[1].text must be string"))?;
    assert!(
        sys_text.contains("Guidance on demand: output !!CHROME!!"),
        "stub must replace the section upstream; got: {sys_text}"
    );
    assert!(
        !sys_text.contains("Never guess tab ids."),
        "section body must be stripped upstream"
    );
    assert!(
        sys_text.contains("# Tone and style\nBe concise."),
        "following section must be untouched"
    );
    assert_eq!(
        upstream["system"][1]["cache_control"]["type"], "ephemeral",
        "cache_control preserved on the stripped block"
    );
    assert!(
        !serde_json::to_string(&upstream)?.contains("<system-reminder>"),
        "no injection without a trigger"
    );

    let requests: Value = client
        .get(format!("{}/api/requests", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let request_id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number"))?;
    let orig: Value = client
        .get(format!(
            "{}/api/requests/{request_id}/body/request",
            proxy.inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    let orig_text = orig["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("original body text must be a string"))?;
    assert!(
        orig_text.contains("Never guess tab ids."),
        "ORIGINAL body must preserve the full section"
    );
    let sent: Value = client
        .get(format!(
            "{}/api/requests/{request_id}/body/request_sent",
            proxy.inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    let sent_text = sent["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("sent body text must be a string"))?;
    assert!(
        sent_text.contains("Guidance on demand"),
        "filtered_request_body must carry the stub rewrite"
    );
    assert!(
        !sent_text.contains("Never guess tab ids."),
        "filtered_request_body must not carry the stripped section"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn lazy_section_trigger_injects_once_per_context_window() -> anyhow::Result<()> {
    let mock_server = lazy_mock_upstream().await;
    let Some(proxy) =
        start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri()).await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    create_lazy_chrome_rule(&client, &proxy.inspect_url, true).await?;
    let session = uuid::Uuid::new_v4().to_string();

    let request_2_messages = json!([
        {"role": "user", "content": [{"type": "text", "text": "open the dashboard page"}]},
        {"role": "assistant", "content": [
            {"type": "text", "text": "Loading browser guidance: !!CHROME!!"},
            {"type": "tool_use", "id": "tu_1", "name": "mcp__claude-in-chrome__tabs_context_mcp", "input": {}}
        ]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "tu_1", "content": "tab group: 1 tab"}
        ]},
    ]);
    let resp2 = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", &session)
        .json(&lazy_request_body(request_2_messages.clone()))
        .send()
        .await?;
    assert_eq!(resp2.status(), 200);

    let mut request_3_messages = request_2_messages
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("messages fixture must be an array"))?;
    request_3_messages.push(json!(
        {"role": "assistant", "content": [{"type": "text", "text": "The dashboard tab is open."}]}
    ));
    request_3_messages.push(json!(
        {"role": "user", "content": [{"type": "text", "text": "now click the export button"}]}
    ));
    let resp3 = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", &session)
        .json(&lazy_request_body(Value::Array(request_3_messages)))
        .send()
        .await?;
    assert_eq!(resp3.status(), 200);

    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 2);
    let upstream_2: Value = serde_json::from_slice(&received[0].body)?;
    let upstream_3: Value = serde_json::from_slice(&received[1].body)?;

    let sys2 = upstream_2["system"][1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("system text must be string"))?;
    assert!(!sys2.contains("Never guess tab ids."), "request 2 stripped");
    let content_2 = upstream_2["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[2].content must be array"))?;
    assert_eq!(content_2.len(), 2, "tool_result + injected reminder");
    let reminder_2 = content_2[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert!(reminder_2.starts_with("<system-reminder>\n"));
    assert!(
        reminder_2.contains("Never guess tab ids."),
        "FULL section text injected"
    );

    let sys3 = upstream_3["system"][1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("system text must be string"))?;
    assert!(
        !sys3.contains("Never guess tab ids."),
        "request 3 still stripped"
    );
    let serialized_3 = serde_json::to_string(&upstream_3)?;
    assert_eq!(
        serialized_3.matches("<system-reminder>").count(),
        1,
        "once-per-window: no re-injection once the marker is no longer the latest assistant turn"
    );
    let content_3 = upstream_3["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[2].content must be array"))?;
    assert_eq!(
        content_3, content_2,
        "the injected block replays byte for byte where it was injected, so the cached prefix holds"
    );
    let newest_3 = upstream_3["messages"][4]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages[4].content must be array"))?;
    assert_eq!(newest_3.len(), 1, "the newest user turn gets no reminder");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn lazy_section_disabled_or_heading_absent_forwards_byte_identical() -> anyhow::Result<()> {
    let mock_server = lazy_mock_upstream().await;

    {
        let Some(proxy) =
            start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri())
                .await?
        else {
            return Ok(());
        };
        let client = reqwest::Client::new();
        create_lazy_chrome_rule(&client, &proxy.inspect_url, false).await?;
        let body_with_heading = serde_json::to_vec(&lazy_request_body(json!([
            {"role": "user", "content": [{"type": "text", "text": "hi"}]}
        ])))?;
        let resp = client
            .post(format!("{}/v1/messages", proxy.proxy_url))
            .header("x-api-key", "sk-test")
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .body(body_with_heading.clone())
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        let received = mock_server
            .received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
        let last = received
            .last()
            .ok_or_else(|| anyhow::anyhow!("expected an upstream request"))?;
        assert_eq!(
            last.body, body_with_heading,
            "disabled rule: upstream body must be byte-identical"
        );
    }

    {
        let Some(proxy) =
            start_proxy_with_config_upstream(b"persist_enabled = true\n", &mock_server.uri())
                .await?
        else {
            return Ok(());
        };
        let client = reqwest::Client::new();
        create_lazy_chrome_rule(&client, &proxy.inspect_url, true).await?;
        let body_without_heading = serde_json::to_vec(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([text_block(
                    "You are Claude Code. No chrome section."
                )]))
                .content(json!([text_block("hi")]))
                .build(),
        )?;
        let resp = reqwest::Client::new()
            .post(format!("{}/v1/messages", proxy.proxy_url))
            .header("x-api-key", "sk-test")
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .body(body_without_heading.clone())
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        let received = mock_server
            .received_requests()
            .await
            .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
        let last = received
            .last()
            .ok_or_else(|| anyhow::anyhow!("expected an upstream request"))?;
        assert_eq!(
            last.body, body_without_heading,
            "heading absent: upstream body must be byte-identical"
        );
    }
    Ok(())
}
