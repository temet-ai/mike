use super::{Candidate, Context, apply, is_held};
use crate::llm::decisions::point::Decider;

pub async fn keep_rows<T: Send + Sync>(
    decider: Option<&Decider>,
    context: &Context<'_>,
    mut rows: Vec<T>,
    candidate: for<'a> fn(&'a T) -> Candidate<'a>,
) -> Vec<T> {
    let Some(decider) = decider else {
        return rows;
    };
    rows.retain(|row| !is_held(context.session, candidate(row).id));
    if rows.is_empty() {
        return rows;
    }
    let kept = {
        let asked: Vec<Candidate<'_>> = rows.iter().map(candidate).collect();
        apply(decider, context, &asked).await
    };
    rows.into_iter()
        .filter(|row| kept.contains(&candidate(row).id))
        .collect()
}
