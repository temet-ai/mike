use super::*;
use crate::config::ProxyConfig;
use anyhow::Context;
use axum::Router;
use axum::routing::get;

const REPORT: &str = r#"{
  "session": "S",
  "continues": null,
  "mod_steps": 3,
  "wire_requests": 4,
  "matched": 2,
  "only_in_mod": [{"turn": "t1", "index": 7, "model": "claude-opus-5-5", "agent": null,
    "usage": {"input_tokens": 2, "output_tokens": 1703, "cache_read_input_tokens": 125755, "cache_creation_input_tokens": 937}}],
  "only_on_wire": [{"request_id": "708212", "origin": "quota_probe", "model": "claude-haiku-4-5", "usage": null},
    {"request_id": "708719", "origin": "primary", "model": "claude-opus-5-5", "usage": null}],
  "only_on_wire_by_origin": {"primary": 1, "quota_probe": 1}
}"#;

fn token_file(dir: &tempfile::TempDir, token: &str) -> anyhow::Result<PathBuf> {
    let path = dir.path().join("operator-token");
    std::fs::write(&path, token)?;
    Ok(path)
}

async fn stand_in(router: Router) -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move { axum::serve(listener, router).await });
    Ok(format!("http://{addr}"))
}

#[tokio::test]
async fn non_uuid_is_bad_request() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let got = get_session_reconcile(State(state), Path("not-a-uuid".into())).await;
    assert_eq!(got.err(), Some(StatusCode::BAD_REQUEST));
    Ok(())
}

#[tokio::test]
async fn passes_tethers_report_through_with_the_operator_token() -> anyhow::Result<()> {
    let router = Router::new().route(
        "/v1/features/reconcile/sessions/{id}",
        get(|headers: axum::http::HeaderMap| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer op-token")
            {
                return (StatusCode::UNAUTHORIZED, String::new());
            }
            (StatusCode::OK, REPORT.to_owned())
        }),
    );
    let dir = tempfile::tempdir()?;
    let endpoint = TetherEndpoint {
        base: stand_in(router).await?,
        token_path: Some(token_file(&dir, "op-token\n")?),
    };
    let answer = fetch(&endpoint, Uuid::new_v4()).await;
    assert!(answer.available, "{answer:?}");
    let report = answer
        .report
        .context("an available answer carries the report")?;
    assert_eq!(report.only_in_mod[0].index, 7);
    assert_eq!(report.only_on_wire.len(), 2);
    assert_eq!(report.only_on_wire[1].usage, None);
    Ok(())
}

#[tokio::test]
async fn tether_not_running_is_unavailable_not_an_error() -> anyhow::Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    drop(listener);
    let dir = tempfile::tempdir()?;
    let endpoint = TetherEndpoint {
        base,
        token_path: Some(token_file(&dir, "t")?),
    };
    let answer = fetch(&endpoint, Uuid::new_v4()).await;
    assert!(!answer.available);
    assert!(answer.reason.unwrap_or_default().contains("not running"));
    Ok(())
}

#[tokio::test]
async fn missing_token_is_unavailable() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let endpoint = TetherEndpoint {
        base: DEFAULT_TETHER_URL.into(),
        token_path: Some(dir.path().join("absent")),
    };
    let answer = fetch(&endpoint, Uuid::new_v4()).await;
    assert!(!answer.available);
    assert!(answer.reason.unwrap_or_default().contains("token"));
    Ok(())
}

#[tokio::test]
async fn a_tether_error_status_is_unavailable_with_its_words() -> anyhow::Result<()> {
    let router = Router::new().route(
        "/v1/features/reconcile/sessions/{id}",
        get(|| async { (StatusCode::BAD_GATEWAY, "the wire capture: mike: 404") }),
    );
    let dir = tempfile::tempdir()?;
    let endpoint = TetherEndpoint {
        base: stand_in(router).await?,
        token_path: Some(token_file(&dir, "t")?),
    };
    let answer = fetch(&endpoint, Uuid::new_v4()).await;
    assert!(!answer.available);
    assert!(
        answer
            .reason
            .unwrap_or_default()
            .contains("the wire capture")
    );
    Ok(())
}
