use sqlx::PgPool;

use crate::store::persistence::PersistError;

#[derive(Debug, sqlx::FromRow)]
pub struct SessionHeadRow {
    pub session_id: String,
    pub last_status: Option<String>,
    pub newest_request_id: Option<i64>,
    pub newest_request_number: Option<i64>,
    pub originating_operator_prompt: Option<String>,
}

pub async fn fetch_session_head(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Option<SessionHeadRow>, PersistError> {
    let row = sqlx::query_file_as!(
        SessionHeadRow,
        "src/store/persistence/rehydrate/session_head.sql",
        session_db_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[derive(Debug, sqlx::FromRow)]
pub struct PromptKindCountRow {
    pub kind_code: String,
    pub turn_count: i64,
}

pub async fn fetch_session_prompt_kind_counts(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<PromptKindCountRow>, PersistError> {
    let rows = sqlx::query_as!(
        PromptKindCountRow,
        r#"SELECT pk.code AS "kind_code!", COUNT(pkc.request_id)::bigint AS "turn_count!"
           FROM prompt_kinds.kinds pk
           LEFT JOIN prompt_kinds.classifications pkc
                  ON pkc.kind_id = pk.id AND pkc.session_id = $1
          GROUP BY pk.code
          ORDER BY pk.code"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
