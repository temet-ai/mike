use super::guard::Guard;
use super::items::{Env, embed_located, group_created, new_statements, split_file};
use super::llm::{ConsolidationLlm, Embedder, Failure};
use super::ranges::worth_splitting;
use super::tally::Tally;
use crate::store::persistence::consolidation::ClaimedItem;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::Utc;

fn db(error: crate::store::persistence::PersistError) -> Failure {
    Failure::transient(error.to_string())
}

pub async fn carry_over<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
    body: &str,
) -> Result<(), Failure> {
    let knowledge = env.persist.knowledge();
    let rehomed = knowledge
        .rehome_document_statements(item.item_id, body, Utc::now() + env.settings.settle)
        .await
        .map_err(db)?;
    tally.statements_gone += rehomed.gone as u32;
    tally.statements_kept += rehomed.rehomed.len() as u32;
    let uncovered: Vec<(usize, usize)> = rehomed
        .uncovered
        .iter()
        .map(|r| (r.start.max(0) as usize, r.end.max(0) as usize))
        .collect();
    let ranges = worth_splitting(body, &uncovered);
    if !ranges.is_empty() {
        let located = split_file(env, guard, body, &ranges).await?;
        let embedded = embed_located(env, guard, &located, true).await?;
        if !embedded.statements.is_empty() {
            let new = new_statements(env, item, embedded.statements);
            let created = knowledge
                .add_item_statements(item.item_id, &new)
                .await
                .map_err(db)?;
            group_created(env, tally, item, &created, &embedded.vectors).await?;
        }
    }
    knowledge
        .stamp_item_split_hash(item.item_id, &item.source_hash)
        .await
        .map_err(db)?;
    Ok(())
}
