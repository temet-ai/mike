use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use futures::StreamExt;
use mike_codex::responses::ResponsesSummary;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::record::{ExchangeRecord, ExchangeStatus, Finished, manifest, spawn_finish};
use super::request_body::RequestBody;
use super::spool::{BodyFile, BodyWriter, Spool};
use super::tee::Tee;
use super::{ROUTE_PREFIX, ResponsesState, correlate, headers};
use crate::extract::headers::{filter_request_headers, filter_response_headers};

const REFUSED_WEBSOCKET: &str = "mike captures Codex Responses traffic over HTTP; the WebSocket transport is refused so Codex falls back to HTTP/SSE";

pub async fn handle(State(state): State<ResponsesState>, request: Request) -> Response {
    super::recovery::maybe_start(&state);
    let (parts, body) = request.into_parts();
    let (mut record, request_writer, response_writer) = match prepare(&state, &parts).await {
        Ok(prepared) => prepared,
        Err(error) => return capture_unavailable(&error),
    };
    let request = RequestBody::new(body.into_data_stream().boxed(), request_writer);
    super::record::prepare_identity(&state, &mut record).await;
    if let Err(error) = record.spool.write_manifest(&manifest(&record, None)).await {
        drop(request);
        return failure(
            state,
            record,
            response_writer,
            format!("capture identity manifest failed: {error}"),
        );
    }
    if headers::is_websocket_upgrade(&parts.headers) {
        return refuse_websocket(state, record, request, response_writer).await;
    }
    let body = match stage_request(&record, request).await {
        Ok(body) => body,
        Err(error) => {
            return failure(state, record, response_writer, error);
        }
    };
    forward(
        state,
        record,
        parts.method,
        &parts.headers,
        body,
        response_writer,
    )
    .await
}

fn capture_unavailable(error: &str) -> Response {
    tracing::error!(error, "Codex request could not enter capture");
    plain(
        StatusCode::SERVICE_UNAVAILABLE,
        &format!("Codex capture unavailable: {error}"),
    )
}

async fn stage_request(
    record: &ExchangeRecord,
    request: RequestBody,
) -> Result<reqwest::Body, String> {
    receive_request(request)
        .await
        .map_err(|error| format!("request capture failed: {error}"))?;
    record
        .request_body
        .wait()
        .await
        .map_err(|error| format!("request capture failed: {error}"))?;
    forwarded_body(&record.request_body.path)
        .await
        .map_err(|error| format!("captured request unavailable: {error}"))
}

async fn receive_request(mut request: RequestBody) -> Result<(), axum::Error> {
    while let Some(chunk) = request.next().await {
        chunk?;
    }
    Ok(())
}

async fn forwarded_body(path: &std::path::Path) -> Result<reqwest::Body, std::io::Error> {
    let file = tokio::fs::File::open(path).await?;
    let stream = futures::stream::try_unfold(file, |mut file| async move {
        let mut bytes = vec![0; mike_codex::capture::MAX_CHUNK_BYTES];
        let read = file.read(&mut bytes).await?;
        bytes.truncate(read);
        Ok::<_, std::io::Error>((read > 0).then(|| (bytes::Bytes::from(bytes), file)))
    });
    Ok(reqwest::Body::wrap_stream(stream))
}

async fn prepare(
    state: &ResponsesState,
    parts: &axum::http::request::Parts,
) -> Result<(ExchangeRecord, BodyWriter, BodyWriter), String> {
    let path = parts.uri.path_and_query().map_or_else(
        || parts.uri.path().to_string(),
        |pq| pq.as_str().to_string(),
    );
    let (launch_session, _) = route_path(&path)?;
    let expected = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let root = state
        .capture_root
        .as_deref()
        .ok_or("capture directory is unavailable")?;
    let spool = Spool::create(root)
        .await
        .map_err(|error| error.to_string())?;
    let (request_body, request_writer) = BodyFile::start(&spool, "request", expected, &state.tasks)
        .await
        .map_err(|error| error.to_string())?;
    let (response_body, response_writer) = BodyFile::start(&spool, "response", None, &state.tasks)
        .await
        .map_err(|error| error.to_string())?;
    let auth_route = headers::auth_route(&parts.headers);
    let mut correlation = correlate(&parts.headers, &[]);
    correlation.session_uuid = None;
    let record = ExchangeRecord {
        correlation,
        auth_route,
        websocket_refused: false,
        method: parts.method.as_str().to_string(),
        upstream_url: upstream_url(state.upstreams.base_for(auth_route), &path),
        path,
        request_headers: headers::captured(&parts.headers),
        forwarded_headers: serde_json::Value::Array(Vec::new()),
        request_body,
        response_body,
        request_body_encoding: headers::text(&parts.headers, "content-encoding"),
        model: None,
        response_status: None,
        response_headers: None,
        response_is_event_stream: false,
        upstream_request_id: None,
        created_at: Utc::now(),
        codex_home: headers::text(&parts.headers, "x-mike-codex-home")
            .filter(|home| home.len() <= 4096)
            .map(Into::into),
        launch_session,
        spool,
    };
    record
        .spool
        .write_manifest(&manifest(&record, None))
        .await?;
    Ok((record, request_writer, response_writer))
}

fn route_path(path: &str) -> Result<(Option<Uuid>, &str), String> {
    let rest = path.strip_prefix(ROUTE_PREFIX).unwrap_or(path);
    if let Some(launch) = rest.strip_prefix("/launch/") {
        let (text, _) = launch
            .split_once('/')
            .ok_or("Codex launch route needs an upstream path")?;
        let id = Uuid::parse_str(text).map_err(|_| "invalid Codex launch session ID")?;
        return Ok((Some(id), &launch[text.len()..]));
    }
    Ok((None, rest))
}

pub fn upstream_url(base: &str, path_and_query: &str) -> String {
    let rest = route_path(path_and_query)
        .map(|(_, rest)| rest)
        .unwrap_or(path_and_query);
    format!("{}{}", base.trim_end_matches('/'), rest)
}

async fn refuse_websocket(
    state: ResponsesState,
    mut record: ExchangeRecord,
    mut request: RequestBody,
    mut writer: BodyWriter,
) -> Response {
    while let Some(chunk) = request.next().await {
        if let Err(error) = chunk {
            return failure(
                state,
                record,
                writer,
                format!("request capture failed: {error}"),
            );
        }
    }
    record.websocket_refused = true;
    record.response_status = Some(StatusCode::UPGRADE_REQUIRED.as_u16());
    record.response_headers = Some(serde_json::json!([[
        "content-type",
        "text/plain; charset=utf-8"
    ]]));
    if let Err(error) = writer.submit(bytes::Bytes::from_static(REFUSED_WEBSOCKET.as_bytes())) {
        return failure(
            state,
            record,
            writer,
            format!("response capture failed: {error}"),
        );
    }
    let written = futures::future::poll_fn(|cx| writer.poll_written(cx)).await;
    if let Some(Err(error)) = written {
        return failure(
            state,
            record,
            writer,
            format!("response capture failed: {error}"),
        );
    }
    writer.finish(true);
    spawn_finish(
        state,
        record,
        Finished {
            status: ExchangeStatus::Refused,
            summary: ResponsesSummary::default(),
            error: Some(REFUSED_WEBSOCKET.into()),
        },
    );
    plain(StatusCode::UPGRADE_REQUIRED, REFUSED_WEBSOCKET)
}

fn upstream_host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| {
            url.host_str().map(|host| {
                url.port()
                    .map_or(host.to_string(), |port| format!("{host}:{port}"))
            })
        })
        .unwrap_or_default()
}

async fn forward(
    state: ResponsesState,
    mut record: ExchangeRecord,
    method: axum::http::Method,
    inbound: &HeaderMap,
    body: reqwest::Body,
    writer: BodyWriter,
) -> Response {
    let outbound = filter_request_headers(inbound, &upstream_host(&record.upstream_url));
    record.forwarded_headers = headers::captured(&outbound);
    if let Err(error) = record.spool.write_manifest(&manifest(&record, None)).await {
        drop(body);
        return failure(
            state,
            record,
            writer,
            format!("capture manifest failed: {error}"),
        );
    }
    match state
        .client
        .request(method, &record.upstream_url)
        .headers(outbound)
        .body(body)
        .send()
        .await
    {
        Ok(upstream) => relay_response(state, record, upstream, writer).await,
        Err(error) => failure(
            state,
            record,
            writer,
            format!("upstream request failed: {error}"),
        ),
    }
}

fn failure(
    state: ResponsesState,
    record: ExchangeRecord,
    mut writer: BodyWriter,
    message: String,
) -> Response {
    writer.finish(false);
    spawn_finish(
        state,
        record,
        Finished {
            status: ExchangeStatus::Error,
            summary: ResponsesSummary::default(),
            error: Some(message.clone()),
        },
    );
    plain(StatusCode::BAD_GATEWAY, &message)
}

async fn relay_response(
    state: ResponsesState,
    mut record: ExchangeRecord,
    upstream: reqwest::Response,
    writer: BodyWriter,
) -> Response {
    let status = upstream.status();
    let response_headers = filter_response_headers(upstream.headers());
    record.response_status = Some(status.as_u16());
    record.response_headers = Some(headers::captured(upstream.headers()));
    record.upstream_request_id = headers::upstream_request_id(upstream.headers());
    record.response_is_event_stream = headers::text(upstream.headers(), "content-type")
        .is_some_and(|value| value.contains("text/event-stream"));
    if let Err(error) = record.spool.write_manifest(&manifest(&record, None)).await {
        return failure(
            state,
            record,
            writer,
            format!("capture manifest failed: {error}"),
        );
    }
    let expected = upstream.content_length();
    let tee = Tee::new(
        upstream.bytes_stream().boxed(),
        state,
        record,
        expected,
        writer,
    );
    let mut response = Response::new(Body::from_stream(tee));
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    response
}

fn plain(status: StatusCode, message: &str) -> Response {
    let mut response = Response::new(Body::from(message.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
