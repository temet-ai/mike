use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptToClassify {
    pub request_id: i64,
    pub session_id: i64,
    pub human_prompt: String,
    pub last_user_message: String,
    pub author: String,
    pub has_agent_id: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalText {
    pub turn_id: i64,
    pub text: Option<String>,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn load_prompts_needing_classification(
        self,
        session_ids: &[i64],
        classifier_version: &str,
    ) -> Result<Vec<PromptToClassify>, PersistError> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("load_prompts_needing_classification", async move {
            Ok(sqlx::query_as!(
                PromptToClassify,
                r#"SELECT r.id AS request_id, r.session_id,
                          r.human_prompt, r.last_user_message,
                          COALESCE(ak.code, 'operator') AS "author!",
                          (r.claude_code_agent_id IS NOT NULL) AS "has_agent_id!"
                     FROM requests.requests r
                     JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
                     JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                                 AND o.code IN ('primary', 'primary_first')
                     LEFT JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
                     LEFT JOIN prompt_kinds.classifications c ON c.request_id = r.id
                    WHERE r.session_id = ANY($1)
                      AND (c.id IS NULL OR c.classifier_version IS DISTINCT FROM $2)
                    ORDER BY r.id"#,
                session_ids,
                classifier_version,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn load_final_texts(self, turn_ids: &[i64]) -> Result<Vec<FinalText>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("load_final_texts", async move {
            Ok(sqlx::query_as!(
                FinalText,
                r#"SELECT t.id AS turn_id, t.final_assistant_text AS text
                     FROM memory.operator_turns t
                    WHERE t.id = ANY($1)
                    ORDER BY t.id"#,
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
