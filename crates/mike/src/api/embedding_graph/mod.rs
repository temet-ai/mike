mod project;
mod session;

pub(crate) use project::get_project_embedding_graph;
pub(crate) use session::{EmbeddingStatusDto, get_session_embedding_graph};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct GraphEdgeDto {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) similarity: f64,
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}
