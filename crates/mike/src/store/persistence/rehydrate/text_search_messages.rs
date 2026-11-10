use sqlx::PgPool;

use crate::store::persistence::PersistError;

#[derive(Debug)]
pub(crate) struct FirstAppearanceMessage {
    pub(crate) request_id: i64,
    pub(crate) ord: i32,
    pub(crate) text: String,
}

pub(crate) async fn find_first_appearance_messages(
    pool: &PgPool,
    session_db_id: i64,
    request_ids: &[i64],
    needle: &str,
    case_sensitive: bool,
) -> Result<Vec<FirstAppearanceMessage>, PersistError> {
    if request_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_file!(
        "src/store/persistence/rehydrate/find_text_message_window.sql",
        session_db_id,
        request_ids,
        needle,
        case_sensitive,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| FirstAppearanceMessage {
            request_id: row.request_id,
            ord: row.ord,
            text: row.text,
        })
        .collect())
}

pub(crate) async fn find_own_search_request_ids(
    pool: &PgPool,
    request_ids: &[i64],
    needle: &str,
    case_sensitive: bool,
    tool_name_suffixes: &[String],
) -> Result<Vec<i64>, PersistError> {
    if request_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_file_scalar!(
        "src/store/persistence/rehydrate/find_text_own_search_requests.sql",
        request_ids,
        tool_name_suffixes,
        needle,
        case_sensitive,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
