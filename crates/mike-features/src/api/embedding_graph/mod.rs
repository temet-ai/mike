pub mod project;
pub mod session;

pub use project::get_project_embedding_graph;
pub use session::{EmbeddingStatusDto, get_session_embedding_graph};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GraphEdgeDto {
    pub from: String,
    pub to: String,
    pub similarity: f64,
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}
