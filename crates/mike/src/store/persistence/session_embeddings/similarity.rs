use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionNeighbour {
    pub session_id: Uuid,
    pub title: Option<String>,
    pub project_id: i64,
    pub project_path: String,
    pub outcome: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub last_status: Option<String>,
    pub similarity: f64,
    pub source: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectCentroidStatus {
    pub present: bool,
    pub session_count: Option<i32>,
    pub embedded_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SiblingProject {
    pub project_id: i64,
    pub project_path: String,
    pub similarity: f64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectGraphNode {
    pub session_id: Uuid,
    pub title: Option<String>,
    pub outcome: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub last_status: Option<String>,
    pub similarity_to_centroid: Option<f64>,
    pub source: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectGraphEdge {
    pub from_session_id: Uuid,
    pub to_session_id: Uuid,
    pub similarity: f64,
}

impl PersistHandle {
    pub async fn refresh_project_centroids(&self) -> Result<usize, PersistError> {
        crate::metrics::time_db("refresh_project_centroids", async move {
            let written = sqlx::query_file_scalar!(
                "src/store/persistence/session_embeddings/refresh_project_centroids.sql"
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(written.len())
        })
        .await
    }

    pub async fn nearest_sessions(
        &self,
        session_id: Uuid,
        k: i64,
        same_project: bool,
        min_similarity: f64,
    ) -> Result<Vec<SessionNeighbour>, PersistError> {
        crate::metrics::time_db("nearest_sessions", async move {
            let rows = if same_project {
                sqlx::query_file_as!(
                    SessionNeighbour,
                    "src/store/persistence/session_embeddings/nearest_sessions_same_project.sql",
                    session_id,
                    k,
                    min_similarity,
                )
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_file_as!(
                    SessionNeighbour,
                    "src/store/persistence/session_embeddings/nearest_sessions.sql",
                    session_id,
                    k,
                    min_similarity,
                )
                .fetch_all(&self.pool)
                .await?
            };
            Ok(rows)
        })
        .await
    }

    pub async fn pairwise_session_similarities(
        &self,
        session_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, Uuid, f64)>, PersistError> {
        crate::metrics::time_db("pairwise_session_similarities", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/session_embeddings/pairwise_similarities.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.a, r.b, r.similarity)).collect())
        })
        .await
    }

    pub async fn project_session_graph(
        &self,
        project_id: i64,
        limit: i64,
        min_similarity: f64,
        per_node_k: i64,
    ) -> Result<(Vec<ProjectGraphNode>, Vec<ProjectGraphEdge>), PersistError> {
        crate::metrics::time_db("project_session_graph", async move {
            let nodes = sqlx::query_file_as!(
                ProjectGraphNode,
                "src/store/persistence/session_embeddings/project_graph_nodes.sql",
                project_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;

            let edges = sqlx::query_file_as!(
                ProjectGraphEdge,
                "src/store/persistence/session_embeddings/project_graph_edges.sql",
                project_id,
                limit,
                min_similarity,
                per_node_k,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok((nodes, edges))
        })
        .await
    }

    pub async fn project_centroid_status(
        &self,
        project_id: i64,
    ) -> Result<ProjectCentroidStatus, PersistError> {
        crate::metrics::time_db("project_centroid_status", async move {
            let row = sqlx::query!(
                r#"SELECT session_count AS "session_count?",
                          embedded_at AS "embedded_at?"
                     FROM memory.project_embeddings
                    WHERE project_id = $1"#,
                project_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(match row {
                Some(r) => ProjectCentroidStatus {
                    present: true,
                    session_count: r.session_count,
                    embedded_at: r.embedded_at,
                },
                None => ProjectCentroidStatus {
                    present: false,
                    session_count: None,
                    embedded_at: None,
                },
            })
        })
        .await
    }

    pub async fn sibling_projects(
        &self,
        project_id: i64,
        k: i64,
    ) -> Result<Vec<SiblingProject>, PersistError> {
        crate::metrics::time_db("sibling_projects", async move {
            let rows = sqlx::query_file_as!(
                SiblingProject,
                "src/store/persistence/session_embeddings/sibling_projects.sql",
                project_id,
                k,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_project_path(
        &self,
        project_id: i64,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("fetch_project_path", async move {
            let project_path = sqlx::query_scalar!(
                "SELECT project_path FROM projects.projects WHERE id = $1",
                project_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(project_path)
        })
        .await
    }
}
