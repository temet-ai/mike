use crate::store::persistence::PersistError;

#[derive(Default)]
pub struct SemanticHits {
    pub references: Vec<String>,
    pub scores: Vec<f64>,
}

pub struct SemanticAsk<'a> {
    pub project_id: i64,
    pub literal: &'a str,
    pub model_keys: &'a [String],
    pub model_name: &'a str,
    pub cut: [f64; 2],
    pub nearest_first: bool,
    pub caller: Option<uuid::Uuid>,
    pub kinds: Option<&'a [String]>,
}

pub async fn semantic_hits(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ask: SemanticAsk<'_>,
) -> Result<SemanticHits, PersistError> {
    plan_mode(tx, false).await?;
    let rows = sqlx::query_file!(
        "src/store/persistence/project_knowledge/semantic.sql",
        ask.project_id,
        ask.literal,
        ask.model_keys,
        ask.model_name,
        &ask.cut[..],
        ask.nearest_first,
        ask.caller,
        ask.kinds,
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .fold(SemanticHits::default(), |mut hits, row| {
            hits.references.push(row.reference);
            hits.scores.push(row.score);
            hits
        }))
}

pub async fn plan_mode(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    generic: bool,
) -> Result<(), PersistError> {
    let mode = if generic {
        "force_generic_plan"
    } else {
        "force_custom_plan"
    };
    sqlx::query!(
        "SELECT set_config('plan_cache_mode', $1, true) AS \"mode!\"",
        mode
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}
