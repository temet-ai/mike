use sqlx::PgPool;
use tracing::warn;

const ASKING: &[&str] = &["should i", "want me to", "do you want"];
const EARLIER_TURNS: i64 = 2;
const EARLIER_TURN_CHARS: i32 = 300;

const EARLIER_TURNS_SQL: &str = "SELECT right(final_assistant_text, $3)
   FROM memory.operator_turns
  WHERE session_id = $1
    AND id < $2
    AND btrim(COALESCE(final_assistant_text, '')) <> ''
  ORDER BY created_at DESC, id DESC
  LIMIT $4";

fn is_closing(c: char) -> bool {
    c.is_whitespace() || matches!(c, '*' | '_' | '`' | '"' | '\'' | ')' | '\u{201d}')
}

pub fn ends_with_operator_question(turn_end: &str) -> bool {
    let tail = turn_end.trim_end_matches(is_closing);
    if tail.ends_with('?') {
        return true;
    }
    let body = tail.trim_end_matches(['.', '!']);
    let last = body
        .rsplit(['.', '!', '?', '\n'])
        .next()
        .unwrap_or(body)
        .to_lowercase();
    ASKING.iter().any(|phrase| last.contains(phrase))
}

pub async fn earlier_turns(pool: &PgPool, session_db_id: i64, turn_id: i64) -> Vec<String> {
    let rows: Result<Vec<String>, sqlx::Error> = sqlx::query_scalar(EARLIER_TURNS_SQL)
        .bind(session_db_id)
        .bind(turn_id)
        .bind(EARLIER_TURN_CHARS)
        .bind(EARLIER_TURNS)
        .fetch_all(pool)
        .await;
    match rows {
        Ok(mut texts) => {
            texts.reverse();
            texts
        }
        Err(error) => {
            warn!(
                %error,
                session_db_id,
                turn_id,
                "steerer checklist: earlier turns read failed; Jev sees no earlier turns"
            );
            Vec::new()
        }
    }
}
