use wiremock::ResponseTemplate;

pub(super) fn infinity_response(count: usize) -> serde_json::Value {
    let vec: Vec<f32> = vec![0.0_f32; 768];
    let data: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            serde_json::json!({
                "embedding": vec,
                "index": i,
                "object": "embedding"
            })
        })
        .collect();
    serde_json::json!({ "data": data })
}

pub(super) fn echo_count_responder() -> impl wiremock::Respond {
    |req: &wiremock::Request| {
        let count = serde_json::from_slice::<serde_json::Value>(&req.body)
            .ok()
            .and_then(|v| v["input"].as_array().map(|a| a.len()))
            .unwrap_or(1);
        ResponseTemplate::new(200).set_body_json(infinity_response(count))
    }
}

pub(super) fn selective_failure_responder(fail_marker: &'static str) -> impl wiremock::Respond {
    move |req: &wiremock::Request| {
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let has_marker = body["input"]
            .as_array()
            .map(|inputs| {
                inputs
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.contains(fail_marker)))
            })
            .unwrap_or(false);
        if has_marker {
            return ResponseTemplate::new(500).set_body_string("synthetic embed failure");
        }
        let count = body["input"].as_array().map(|a| a.len()).unwrap_or(1);
        ResponseTemplate::new(200).set_body_json(infinity_response(count))
    }
}

pub(super) async fn patch_embedding_setting(
    inspect_url: &str,
    key: &str,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("{inspect_url}/api/settings/{key}"))
        .json(&serde_json::json!({ "value": value }))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "failed to patch {key}: {}",
        resp.status()
    );
    Ok(())
}

pub(super) fn vec_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 12 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}
