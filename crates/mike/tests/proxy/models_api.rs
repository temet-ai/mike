use crate::common;

use common::start_proxy_with_openrouter;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn models_api_maps_pricing_and_fields() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "data": [
                        {
                            "id": "openai/gpt-5.6-luna",
                            "name": "GPT 5.6 Luna",
                            "created": 1780000000,
                            "description": "OpenAI's GPT 5.6 Luna multimodal model.",
                            "context_length": 128000,
                            "pricing": {
                                "prompt": "0.000005",
                                "completion": "0.000015"
                            },
                            "architecture": {
                                "input_modalities": ["text", "image"]
                            },
                            "supported_parameters": ["tools", "structured_outputs", "reasoning"]
                        },
                        {
                            "id": "meta-llama/llama-3.1-8b-instruct:free",
                            "name": "Llama 3.1 8B Instruct (free)",
                            "created": 1780000000,
                            "context_length": 131072,
                            "pricing": {
                                "prompt": "0",
                                "completion": "0"
                            }
                        }
                    ]
                })),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/models"))
        .send()
        .await?;

    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = resp.json().await?;
    let models = body["models"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("models must be an array, got {:?}", body["models"]))?;
    assert_eq!(models.len(), 2, "expected 2 models");

    assert_luna_pricing_and_fields(&models[0])?;
    assert_llama_free_model_fields(&models[1])?;

    shutdown.cancel();
    Ok(())
}

fn assert_luna_pricing_and_fields(luna: &serde_json::Value) -> anyhow::Result<()> {
    assert_eq!(luna["id"], "openai/gpt-5.6-luna");
    assert_eq!(luna["name"], "GPT 5.6 Luna");
    assert_eq!(luna["context_length"], 128000);
    let prompt_price = luna["prompt_price_per_1m"].as_f64().ok_or_else(|| {
        anyhow::anyhow!(
            "prompt_price_per_1m must be a number, got {:?}",
            luna["prompt_price_per_1m"]
        )
    })?;
    assert!(
        (prompt_price - 5.0).abs() < 1e-6,
        "expected prompt_price_per_1m=5.0 for luna, got {prompt_price}"
    );
    let completion_price = luna["completion_price_per_1m"].as_f64().ok_or_else(|| {
        anyhow::anyhow!(
            "completion_price_per_1m must be a number, got {:?}",
            luna["completion_price_per_1m"]
        )
    })?;
    assert!(
        (completion_price - 15.0).abs() < 1e-6,
        "expected completion_price_per_1m=15.0 for luna, got {completion_price}"
    );
    assert_eq!(
        luna["description"], "OpenAI's GPT 5.6 Luna multimodal model.",
        "description must pass through from OpenRouter unabridged"
    );
    assert_eq!(
        luna["input_modalities"],
        serde_json::json!(["text", "image"]),
        "input_modalities must come from architecture.input_modalities"
    );
    assert_eq!(
        luna["supports_tools"], true,
        "luna advertises 'tools' in supported_parameters"
    );
    assert_eq!(
        luna["supports_reasoning"], true,
        "luna advertises 'reasoning' in supported_parameters"
    );
    Ok(())
}

fn assert_llama_free_model_fields(llama: &serde_json::Value) -> anyhow::Result<()> {
    assert_eq!(llama["id"], "meta-llama/llama-3.1-8b-instruct:free");
    assert_eq!(
        llama["description"], "",
        "description defaults to empty string when OpenRouter omits it"
    );
    assert_eq!(
        llama["input_modalities"],
        serde_json::json!([]),
        "input_modalities defaults to empty when architecture is omitted"
    );
    assert_eq!(
        llama["supports_tools"], false,
        "supports_tools defaults to false when supported_parameters is omitted"
    );
    assert_eq!(
        llama["supports_reasoning"], false,
        "supports_reasoning defaults to false when supported_parameters is omitted"
    );
    let llama_prompt_price = llama["prompt_price_per_1m"]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("free model prompt_price_per_1m must be a number"))?;
    assert!(
        llama_prompt_price.abs() < 1e-9,
        "free model prompt price must be 0"
    );
    Ok(())
}

#[tokio::test]
async fn models_api_returns_502_on_upstream_failure() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/models"))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_GATEWAY,
        "expected 502 when OpenRouter is down; got {}",
        resp.status()
    );

    shutdown.cancel();
    Ok(())
}
