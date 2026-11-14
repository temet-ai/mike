use super::super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct PrelabelTurn {
    pub turn_id: i64,
    pub session_id: i64,
    pub user_prompt: String,
    pub final_text: String,
    pub next_prompt: String,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn prelabel_turns(self, ids: &[i64]) -> Result<Vec<PrelabelTurn>, PersistError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = ids.to_vec();
        crate::metrics::time_db("prelabel_turns", async move {
            Ok(sqlx::query_as!(
                PrelabelTurn,
                r#"SELECT t.id AS "turn_id!", t.session_id AS "session_id!",
                          COALESCE(t.user_prompt, '') AS "user_prompt!",
                          COALESCE(t.final_assistant_text, '') AS "final_text!",
                          COALESCE(n.user_prompt, '') AS "next_prompt!"
                     FROM memory.operator_turns t
                     LEFT JOIN memory.turn_facts f ON f.operator_turn_id = t.id
                     LEFT JOIN memory.operator_turns n ON n.id = f.next_genuine_turn_id
                    WHERE t.id = ANY($1::bigint[])"#,
                &ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
