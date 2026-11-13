use super::dedupe::Embedded;
use super::relate::{self, NewLessonContext, WriteTimeVerdict};
use super::{ProposerDeps, ProposerReport, WriteOutcome, embedding_for_rule, guidance, write};
use crate::store::persistence::lessons::proposals::queue::SavedProposal;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

pub(super) async fn retry_pending(deps: &ProposerDeps<'_>, report: &mut ProposerReport) {
    let pending = match deps.persist.lessons().pending_lesson_proposals(40).await {
        Ok(pending) => pending,
        Err(error) => {
            tracing::warn!(%error, "lesson_proposer: failed to load pending proposals");
            report.errors += 1;
            return;
        }
    };
    for saved in pending {
        retry_one(deps, report, &saved).await;
    }
}

async fn retry_one(deps: &ProposerDeps<'_>, report: &mut ProposerReport, saved: &SavedProposal) {
    match apply_saved(deps, saved).await {
        Ok(outcome) => report.record(outcome),
        Err(error) => {
            tracing::warn!(%error, verdict_id = saved.candidate.verdict_id,
                "lesson_proposer: replay failed; saved output retained");
            report.errors += 1;
        }
    }
}

async fn related_verdict(
    deps: &ProposerDeps<'_>,
    saved: &SavedProposal,
    rule: &str,
    embedded: &Embedded,
) -> Result<WriteTimeVerdict, String> {
    let neighbours = deps
        .persist
        .lessons()
        .find_lesson_neighbours(
            saved.candidate.project_id,
            &embedded.vector,
            relate::NEIGHBOUR_MAX_DISTANCE,
            relate::NEIGHBOUR_LIMIT,
        )
        .await
        .map_err(|e| e.to_string())?;
    let shown = guidance::closest_to_text(deps, saved.candidate.project_id, rule).await;
    let daily_limit = deps
        .settings
        .quiet
        .effective_limit(deps.persist, deps.settings.max_per_day)
        .await;
    let reserved = deps
        .persist
        .lessons()
        .reserve_lesson_proposer_call(daily_limit)
        .await
        .map_err(|e| e.to_string())?;
    if !reserved {
        return Err("the lesson proposer's daily call budget is spent; the write-time decision waits for the next tick".to_owned());
    }
    let (answer, _usage) =
        relate::call(deps.llm, rule, &neighbours, &shown, deps.settings.max_chars)
            .await
            .map_err(|e| e.to_string())?;
    let context = NewLessonContext {
        verdict_id: saved.candidate.verdict_id,
        source_turn_id: saved.candidate.operator_turn_id,
        max_chars: deps.settings.max_chars,
    };
    let verdict = relate::resolve(&answer, &neighbours, &shown, &context);
    for note in &verdict.notes {
        tracing::warn!(
            verdict_id = saved.candidate.verdict_id,
            note,
            "lesson_proposer: write-time decision adjusted"
        );
    }
    Ok(verdict)
}

fn wants_relation(deps: &ProposerDeps<'_>, saved: &SavedProposal) -> Option<String> {
    if saved.candidate.confidence < deps.settings.min_confidence {
        return None;
    }
    write::accepted_rule(&saved.candidate, &saved.proposed).map(str::to_owned)
}

pub(super) async fn apply_saved(
    deps: &ProposerDeps<'_>,
    saved: &SavedProposal,
) -> Result<WriteOutcome, String> {
    deps.persist
        .lessons()
        .touch_lesson_proposal(saved.candidate.verdict_id)
        .await
        .map_err(|e| e.to_string())?;
    let rule = wants_relation(deps, saved);
    let embedded = embedding_for_rule(deps.embedding, rule.as_deref()).await;
    let verdict = match (rule.as_deref(), &embedded) {
        (Some(rule), Some(embedded)) => Some(related_verdict(deps, saved, rule, embedded).await?),
        _ => None,
    };
    let stored_embedding = match verdict.as_ref().and_then(WriteTimeVerdict::replacement) {
        Some(replacement) => embedding_for_rule(deps.embedding, Some(replacement)).await,
        None => embedded,
    };
    let decision = write::Decision {
        candidate: &saved.candidate,
        proposed: &saved.proposed,
        embedding: stored_embedding.as_ref(),
        verdict: verdict.as_ref(),
        model: &saved.model,
        cost_usd: saved.cost_usd,
        generation_id: saved.generation_id.as_deref(),
        similar: &saved.similar,
    };
    write::decide_and_write(deps.persist, deps.settings, &decision)
        .await
        .map_err(|e| e.to_string())
}
