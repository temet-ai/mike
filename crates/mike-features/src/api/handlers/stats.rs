use super::*;
use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[derive(Serialize)]
pub struct StatsResponse {
    pub request_count: u64,
    pub active_streams: usize,
    pub uptime_seconds: u64,
    pub project_count: Option<i64>,
}

pub async fn stats(State(state): State<BaseState>) -> Json<StatsResponse> {
    let project_count: Option<i64> = if let Some(persist) = state.persist() {
        match persist.launch().count_projects().await {
            Ok(n) => Some(n),
            Err(e) => {
                tracing::warn!(error = %e, "stats: failed to count projects");
                None
            }
        }
    } else {
        None
    };
    Json(StatsResponse {
        request_count: state.request_count.load(Ordering::Relaxed),
        active_streams: state.in_flight_len(),
        uptime_seconds: state.started_at.elapsed().as_secs(),
        project_count,
    })
}

pub async fn metrics(State(state): State<BaseState>) -> Response {
    let body = state.metrics_handle.render();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    (StatusCode::OK, headers, body).into_response()
}
