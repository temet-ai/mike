use uuid::Uuid;

use super::invalid;
use super::locations::ProjectIdentity;
use crate::store::persistence::PersistError;

const NEAR_MATCH_LIMIT: i64 = 5;

struct Candidate {
    id: i64,
    project_uuid: Uuid,
    project_path: String,
    name: String,
}

fn describe(candidates: &[Candidate]) -> String {
    candidates
        .iter()
        .map(|c| format!("{} ({}, {})", c.name, c.project_uuid, c.project_path))
        .collect::<Vec<_>>()
        .join("; ")
}

fn ambiguous_message(selector: &str, candidates: &[Candidate]) -> String {
    format!(
        "Project `{selector}` is ambiguous. It matches: {}. Pass one of these project UUIDs.",
        describe(candidates)
    )
}

fn unknown_message(selector: &str, near: &[Candidate]) -> String {
    if near.is_empty() {
        return format!(
            "Unknown project `{selector}`: it is not a project UUID, numeric id, path, name or folder name, and no project's name or path contains it. Use list_project_locations."
        );
    }
    format!(
        "Unknown project `{selector}`: it is not a project UUID, numeric id, path, name or folder name. Projects whose name or path contains it: {}. Pass one of these project UUIDs.",
        describe(near)
    )
}

fn numeric_id(selector: &str) -> Option<i64> {
    if selector.is_empty() || !selector.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    selector.parse().ok()
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn resolve_knowledge_project(
        self,
        selector: &str,
    ) -> Result<ProjectIdentity, PersistError> {
        match self.find_knowledge_project(selector).await? {
            Some(project) => Ok(project),
            None => Err(self.unknown_project_error(selector).await),
        }
    }

    pub async fn find_knowledge_project(
        self,
        selector: &str,
    ) -> Result<Option<ProjectIdentity>, PersistError> {
        crate::metrics::time_db("knowledge_resolve_project", self.lookup_project(selector)).await
    }

    pub async fn lookup_project(
        self,
        selector: &str,
    ) -> Result<Option<ProjectIdentity>, PersistError> {
        let mut candidates = sqlx::query_file_as!(
            Candidate,
            "src/store/persistence/project_knowledge/resolve.sql",
            selector,
            selector.parse::<Uuid>().ok(),
            numeric_id(selector),
        )
        .fetch_all(self.pool())
        .await?;
        if candidates.len() > 1 {
            return Err(invalid(&ambiguous_message(selector, &candidates)));
        }
        Ok(candidates.pop().map(|c| ProjectIdentity {
            id: c.id,
            project_uuid: c.project_uuid,
            project_path: c.project_path,
        }))
    }

    async fn unknown_project_error(self, selector: &str) -> PersistError {
        let near = sqlx::query_as!(
            Candidate,
            r#"SELECT p.id AS "id!", p.project_uuid AS "project_uuid!",
                      p.project_path AS "project_path!",
                      COALESCE(p.custom_name, p.display_name) AS "name!"
                 FROM projects.projects p
                WHERE p.merged_into_id IS NULL AND p.project_path <> '(no project)'
                  AND (strpos(lower(p.project_path), lower($1)) > 0
                       OR strpos(lower(COALESCE(p.custom_name, p.display_name)), lower($1)) > 0)
                ORDER BY p.id
                LIMIT $2"#,
            selector,
            NEAR_MATCH_LIMIT,
        )
        .fetch_all(self.pool())
        .await;
        match near {
            Ok(near) => invalid(&unknown_message(selector, &near)),
            Err(error) => error.into(),
        }
    }
}
