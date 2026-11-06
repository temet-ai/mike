use serde_json::{Value, json};

use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const MEMORY_SOURCE: &str = "claude-memory";
pub(crate) const SKILLS_SOURCE: &str = "skills";
pub(crate) const RULES_SOURCE: &str = "rules";
pub(crate) const AGENTS_SOURCE: &str = "agents";

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct GuidanceRow {
    pub reference: String,
    pub source_name: String,
    pub relative_path: String,
    pub title: String,
    pub body: String,
    pub metadata: Value,
    pub similarity: Option<f64>,
}

impl GuidanceRow {
    pub fn kind_label(&self) -> &'static str {
        match self.source_name.as_str() {
            SKILLS_SOURCE => "skill",
            RULES_SOURCE => "rule",
            AGENTS_SOURCE => "agent",
            _ => "memory",
        }
    }

    pub fn display_name(&self) -> String {
        let named = self
            .metadata
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        match named {
            Some(name) => name.to_owned(),
            None => self.title.clone(),
        }
    }

    pub fn names_for_citation(&self) -> Vec<String> {
        let path = self.relative_path.to_lowercase();
        let mut names = vec![path.clone()];
        let stem = if self.kind_label() == "skill" {
            path.split('/').next().map(str::to_owned)
        } else {
            path.strip_suffix(".md").map(str::to_owned)
        };
        names.extend(stem.filter(|stem| !stem.is_empty()));
        names.push(self.display_name().to_lowercase());
        names.retain(|name| !name.is_empty());
        names
    }

    pub fn cited_by(&self, reason: &str) -> bool {
        let reason = reason.to_lowercase();
        self.names_for_citation()
            .iter()
            .any(|name| reason.contains(name.as_str()))
    }
}

pub struct ShownGuidance<'a> {
    pub row: &'a GuidanceRow,
    pub cited: bool,
}

impl PersistHandle {
    pub async fn closest_guidance(
        &self,
        project_id: i64,
        query: &str,
        vector: Option<&(String, String)>,
        limit: i64,
    ) -> Result<Vec<GuidanceRow>, PersistError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(sqlx::query_file_as!(
            GuidanceRow,
            "src/store/persistence/project_knowledge/guidance.sql",
            project_id,
            query,
            vector.map(|v| v.0.as_str()),
            vector.map(|v| v.1.as_str()),
            limit.clamp(1, 10)
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn record_proposal_guidance(
        &self,
        verdict_id: i64,
        project_id: i64,
        shown: &[ShownGuidance<'_>],
    ) -> Result<(), PersistError> {
        if shown.is_empty() {
            return Ok(());
        }
        let rows: Vec<Value> = shown
            .iter()
            .map(|item| {
                json!({
                    "reference": item.row.reference,
                    "source_kind": item.row.kind_label(),
                    "name": item.row.display_name(),
                    "relative_path": item.row.relative_path,
                    "similarity": item.row.similarity,
                    "cited": item.cited,
                })
            })
            .collect();
        let rows = Value::Array(rows);
        sqlx::query!(
            "INSERT INTO memory.lesson_proposal_guidance
                (verdict_id, project_id, reference, source_kind, name, relative_path, similarity, cited)
             SELECT $1, $2, x.reference, x.source_kind, x.name, x.relative_path, x.similarity, x.cited
               FROM jsonb_to_recordset($3::jsonb) AS x(
                    reference text, source_kind text, name text, relative_path text,
                    similarity double precision, cited boolean)
             ON CONFLICT (verdict_id, reference) DO UPDATE
                SET similarity = EXCLUDED.similarity, cited = EXCLUDED.cited,
                    name = EXCLUDED.name, relative_path = EXCLUDED.relative_path",
            verdict_id,
            project_id,
            &rows
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
