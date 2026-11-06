use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use metrics_exporter_prometheus::PrometheusBuilder;
use tower::ServiceExt;

use super::{COUNTER_SPA_DELTA_FALLBACK_TOTAL, MAX_BEACON_BYTES, spa_delta_fallback_route};

const REASONS: [&str; 5] = [
    "predecessor-not-loaded",
    "compaction",
    "lineage-broken",
    "chrome-strip-failed",
    "parse-error",
];

async fn send(body: String, content_type: &str) -> anyhow::Result<axum::response::Response> {
    Ok(Router::new()
        .route("/", spa_delta_fallback_route())
        .oneshot(
            Request::post("/")
                .header("content-type", content_type)
                .body(Body::from(body))?,
        )
        .await?)
}

#[tokio::test]
async fn allowed_reasons_return_no_content_and_increment_only_fixed_series() -> anyhow::Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    for reason in REASONS {
        let response = send(format!(r#"{{"reason":"{reason}"}}"#), "application/json").await?;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            to_bytes(response.into_body(), MAX_BEACON_BYTES)
                .await?
                .is_empty()
        );
    }
    let rendered = handle.render();
    let samples: Vec<_> = rendered
        .lines()
        .filter(|line| line.starts_with(COUNTER_SPA_DELTA_FALLBACK_TOTAL))
        .collect();
    assert_eq!(samples.len(), REASONS.len());
    for reason in REASONS {
        assert!(samples.contains(
            &format!("{COUNTER_SPA_DELTA_FALLBACK_TOTAL}{{reason=\"{reason}\"}} 1").as_str()
        ));
    }
    Ok(())
}

#[tokio::test]
async fn invalid_beacons_are_rejected_without_recording_metrics() -> anyhow::Result<()> {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    for body in [
        r#"{"reason":"private-raw-text"}"#,
        r#"{"reason":"compaction","payload":"private-raw-text"}"#,
        r#"{"reason":"compaction","reason":"parse-error"}"#,
        r#"{}"#,
        r#"{"reason":null}"#,
        r#"{"reason":7}"#,
        r#"[]"#,
    ] {
        let response = send(body.to_owned(), "application/json").await?;
        assert!(
            matches!(
                response.status(),
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{body}: {}",
            response.status()
        );
    }
    assert_eq!(
        send("{".to_owned(), "application/json").await?.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(r#"{"reason":"compaction"}"#.to_owned(), "text/plain")
            .await?
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert!(
        !handle
            .render()
            .lines()
            .any(|line| line.starts_with(COUNTER_SPA_DELTA_FALLBACK_TOTAL))
    );
    Ok(())
}

#[tokio::test]
async fn body_limit_accepts_256_bytes_and_rejects_257_without_content_length() -> anyhow::Result<()>
{
    let mut body = r#"{"reason":"compaction"}"#.to_owned();
    body.push_str(&" ".repeat(MAX_BEACON_BYTES - body.len()));
    assert_eq!(
        send(body.clone(), "application/json").await?.status(),
        StatusCode::NO_CONTENT
    );
    body.push(' ');
    assert_eq!(
        send(body, "application/json").await?.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    Ok(())
}
