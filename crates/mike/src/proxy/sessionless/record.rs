use axum::http::{HeaderMap, Method, Uri};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tokio_util::task::TaskTracker;

use crate::config::AppState;
use crate::proxy::headers::captured_header_pairs;
use crate::store::persistence::PersistHandle;

const INSERT: &str = "INSERT INTO requests.sessionless_requests
     (reason, method, path, query, user_agent, request_headers, request_body,
      response_status, response_headers, response_body, response_body_size,
      response_body_refused, status, error, created_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
 RETURNING id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Complete,
    Error,
    Aborted,
}

impl Status {
    fn as_db_str(self) -> &'static str {
        match self {
            Status::Complete => "complete",
            Status::Error => "error",
            Status::Aborted => "aborted",
        }
    }
}

#[derive(Debug)]
pub(super) struct SessionlessRecord {
    reason: &'static str,
    method: String,
    path: String,
    query: Option<String>,
    user_agent: Option<String>,
    request_headers: Value,
    request_body: Bytes,
    response_status: Option<u16>,
    response_headers: Option<Value>,
    created_at: DateTime<Utc>,
}

impl SessionlessRecord {
    pub(super) fn new(
        reason: &'static str,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: Bytes,
    ) -> Self {
        Self {
            reason,
            method: method.as_str().to_string(),
            path: uri.path().to_string(),
            query: uri.query().map(str::to_string),
            user_agent: headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
            request_headers: header_json(headers),
            request_body: body,
            response_status: None,
            response_headers: None,
            created_at: Utc::now(),
        }
    }

    pub(super) fn set_response(&mut self, status: u16, headers: &HeaderMap) {
        self.response_status = Some(status);
        self.response_headers = Some(header_json(headers));
    }
}

#[derive(Debug)]
pub(super) struct Finished {
    pub(super) status: Status,
    pub(super) response_body: Option<Bytes>,
    pub(super) response_body_size: Option<u64>,
    pub(super) refused: bool,
    pub(super) error: Option<String>,
}

impl Finished {
    pub(super) fn failed(error: String) -> Self {
        Self {
            status: Status::Error,
            response_body: None,
            response_body_size: None,
            refused: false,
            error: Some(error),
        }
    }
}

fn header_json(headers: &HeaderMap) -> Value {
    Value::Array(
        captured_header_pairs(headers)
            .into_iter()
            .map(|(name, value)| json!([name, value]))
            .collect(),
    )
}

pub(super) struct Recorder {
    persist: Option<PersistHandle>,
    tracker: TaskTracker,
}

impl Recorder {
    pub(super) fn from_state(state: &AppState) -> Self {
        Self {
            persist: state.persist().cloned(),
            tracker: state.tracker.clone(),
        }
    }

    pub(super) fn record(&self, record: SessionlessRecord, finished: Finished) {
        let Some(persist) = self.persist.clone() else {
            return;
        };
        let task = async move {
            if let Err(error) = insert(&persist, &record, &finished).await {
                tracing::warn!(
                    error = %error,
                    path = %record.path,
                    "sessionless request was not recorded"
                );
            }
        };
        crate::observability::spans::spawn_tracked(&self.tracker, task);
    }
}

async fn insert(
    persist: &PersistHandle,
    record: &SessionlessRecord,
    finished: &Finished,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(INSERT)
        .bind(record.reason)
        .bind(&record.method)
        .bind(&record.path)
        .bind(&record.query)
        .bind(&record.user_agent)
        .bind(&record.request_headers)
        .bind(record.request_body.as_ref())
        .bind(
            record
                .response_status
                .map(|s| i16::try_from(s).unwrap_or(i16::MAX)),
        )
        .bind(&record.response_headers)
        .bind(finished.response_body.as_ref().map(Bytes::as_ref))
        .bind(
            finished
                .response_body_size
                .map(|s| i64::try_from(s).unwrap_or(i64::MAX)),
        )
        .bind(finished.refused)
        .bind(finished.status.as_db_str())
        .bind(&finished.error)
        .bind(record.created_at)
        .fetch_one(persist.pool())
        .await
}
