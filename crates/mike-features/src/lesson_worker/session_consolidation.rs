use crate::analysis::{EmbedError, EmbeddingService};
use crate::store::persistence::consolidation_audit::ConsolidationActionRecord;
use crate::store::persistence::lessons::{LessonRow, SimilarLesson};
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

use super::action_record::{
    promote_failed_action_record, promoted_action_record, runners_up_json, skipped_action_record,
};

pub async fn run_session_consolidation(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    idle_secs: u64,
    dedup_threshold: f32,
) -> bool {
    let Some(project_ids) = discover_idle_projects(persist, idle_secs).await else {
        return false;
    };
    if project_ids.is_empty() {
        log_no_idle_projects();
        return false;
    }
    log_session_starting(project_ids.len());

    let run_id = open_session_run(persist).await;

    let mut any_error = false;
    let mut any_embed_call_failed = false;
    for project_id in project_ids {
        let outcome =
            consolidate_project(persist, embedding, project_id, dedup_threshold, run_id).await;
        any_error |= outcome.hard_failure;
        any_embed_call_failed |= outcome.embed_call_failed;
    }

    finish_session_run_if_open(persist, run_id, any_error).await;

    any_embed_call_failed
}

async fn discover_idle_projects(persist: &PersistHandle, idle_secs: u64) -> Option<Vec<i64>> {
    match persist.lessons().discover_idle_projects(idle_secs).await {
        Ok(ids) => Some(ids),
        Err(e) => {
            log_discover_idle_failed(&e);
            None
        }
    }
}

fn log_discover_idle_failed(e: &PersistError) {
    tracing::warn!(error = %e, "lesson_worker: failed to discover idle projects");
}

fn log_no_idle_projects() {
    tracing::debug!("lesson_worker: no idle projects with raw lessons");
}

fn log_session_starting(project_count: usize) {
    tracing::info!(
        project_count,
        "lesson_worker: session consolidation starting"
    );
}

async fn open_session_run(persist: &PersistHandle) -> Option<i64> {
    match persist
        .lessons()
        .start_consolidation_run("session", "schedule")
        .await
    {
        Ok(id) => Some(id),
        Err(e) => {
            log_open_session_run_failed(&e);
            None
        }
    }
}

fn log_open_session_run_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "lesson_worker: failed to open a consolidation run — proceeding without an audit trail"
    );
}

async fn finish_session_run_if_open(persist: &PersistHandle, run_id: Option<i64>, any_error: bool) {
    let Some(run_id) = run_id else {
        return;
    };
    let status = if any_error { "partial" } else { "completed" };
    if let Err(e) = persist
        .lessons()
        .finish_consolidation_run(run_id, status, None)
        .await
    {
        log_finish_session_run_failed(run_id, &e);
    }
}

fn log_finish_session_run_failed(run_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, run_id, "lesson_worker: failed to close the consolidation run");
}

pub struct ConsolidationOutcome {
    pub hard_failure: bool,
    pub embed_call_failed: bool,
}

pub struct LessonConsolidationCtx {
    pub project_id: i64,
    pub distance_threshold: f32,
    pub model_name: String,
}

#[derive(Default)]
struct ConsolidationTally {
    promoted: u64,
    skipped: u64,
    failed: u64,
    embed_failures: u64,
}

impl ConsolidationTally {
    fn record(&mut self, outcome: LessonOutcome) {
        match outcome {
            LessonOutcome::Promoted => self.promoted += 1,
            LessonOutcome::SkippedEmbedFailure => {
                self.skipped += 1;
                self.embed_failures += 1;
            }
            LessonOutcome::SkippedOtherFailure => self.skipped += 1,
            LessonOutcome::Failed => self.failed += 1,
        }
    }
}

enum LessonOutcome {
    Promoted,
    SkippedEmbedFailure,
    SkippedOtherFailure,
    Failed,
}

pub async fn consolidate_project(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    project_id: i64,
    dedup_threshold: f32,
    run_id: Option<i64>,
) -> ConsolidationOutcome {
    let Some(lessons) = claim_lessons(persist, project_id).await else {
        return ConsolidationOutcome {
            hard_failure: true,
            embed_call_failed: false,
        };
    };

    if lessons.is_empty() {
        return ConsolidationOutcome {
            hard_failure: false,
            embed_call_failed: false,
        };
    }

    log_consolidating(project_id, lessons.len());

    let ctx = LessonConsolidationCtx {
        project_id,
        distance_threshold: 1.0_f32 - dedup_threshold,
        model_name: embedding.model_name(),
    };

    let mut tally = ConsolidationTally::default();
    let mut actions: Vec<ConsolidationActionRecord> = Vec::with_capacity(lessons.len());
    for lesson in lessons {
        let outcome = process_one_lesson(persist, embedding, &ctx, lesson, &mut actions).await;
        tally.record(outcome);
    }

    log_project_complete(project_id, &tally);
    record_actions_if_run_open(persist, run_id, project_id, &actions).await;

    ConsolidationOutcome {
        hard_failure: tally.failed > 0,
        embed_call_failed: tally.embed_failures > 0,
    }
}

async fn claim_lessons(persist: &PersistHandle, project_id: i64) -> Option<Vec<LessonRow>> {
    match persist
        .lessons()
        .claim_raw_lessons_for_consolidation(project_id, 50)
        .await
    {
        Ok(l) => Some(l),
        Err(e) => {
            log_claim_failed(project_id, &e);
            None
        }
    }
}

fn log_claim_failed(project_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, project_id, "lesson_worker: failed to claim raw lessons");
}

fn log_consolidating(project_id: i64, lesson_count: usize) {
    tracing::info!(
        project_id,
        lesson_count,
        "lesson_worker: consolidating raw lessons"
    );
}

fn log_project_complete(project_id: i64, tally: &ConsolidationTally) {
    tracing::info!(
        project_id,
        promoted = tally.promoted,
        skipped = tally.skipped,
        failed = tally.failed,
        embed_failures = tally.embed_failures,
        "lesson_worker: session consolidation complete for project"
    );
}

async fn process_one_lesson(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    ctx: &LessonConsolidationCtx,
    lesson: LessonRow,
    actions: &mut Vec<ConsolidationActionRecord>,
) -> LessonOutcome {
    let Some(vec) = embed_lesson(embedding, ctx, &lesson, actions).await else {
        return LessonOutcome::SkippedEmbedFailure;
    };

    let Some(neighbours) = find_neighbours(persist, ctx, &lesson, &vec, actions).await else {
        return LessonOutcome::SkippedOtherFailure;
    };

    let nearest = neighbours.first();
    log_near_duplicate_if_close(&lesson, nearest, ctx.distance_threshold);
    let runners_up = runners_up_json(&neighbours);

    match persist
        .lessons()
        .promote_lesson_to_daily(lesson.id, &vec, &ctx.model_name)
        .await
    {
        Ok(()) => {
            actions.push(promoted_action_record(&lesson, ctx, nearest, runners_up));
            LessonOutcome::Promoted
        }
        Err(e) => {
            log_promote_failed(lesson.id, &e);
            actions.push(promote_failed_action_record(
                &lesson, &e, ctx, nearest, runners_up,
            ));
            LessonOutcome::Failed
        }
    }
}

fn log_promote_failed(lesson_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, lesson_id = %lesson_id, "lesson_worker: promote_lesson_to_daily failed");
}

async fn embed_lesson(
    embedding: &EmbeddingService,
    ctx: &LessonConsolidationCtx,
    lesson: &LessonRow,
    actions: &mut Vec<ConsolidationActionRecord>,
) -> Option<Vec<f32>> {
    match embedding
        .embed_documents(std::slice::from_ref(&lesson.text))
        .await
    {
        Ok(mut vecs) if !vecs.is_empty() => Some(vecs.remove(0)),
        Ok(_) => {
            log_embed_empty(lesson.id, ctx.project_id);
            actions.push(skipped_action_record(
                lesson,
                ctx,
                "embed_documents returned empty".to_string(),
            ));
            None
        }
        Err(e) => {
            log_embed_failed(lesson.id, ctx.project_id, &e);
            actions.push(skipped_action_record(
                lesson,
                ctx,
                format!("embed failed: {e}"),
            ));
            None
        }
    }
}

fn log_embed_empty(lesson_id: i64, project_id: i64) {
    tracing::warn!(
        lesson_id = %lesson_id,
        project_id,
        "lesson_worker: embed_documents returned empty — lesson stays raw, will retry next tick"
    );
}

fn log_embed_failed(lesson_id: i64, project_id: i64, e: &EmbedError) {
    tracing::warn!(
        error = %e,
        lesson_id = %lesson_id,
        project_id,
        "lesson_worker: embed failed — lesson stays raw, will retry next tick"
    );
}

async fn find_neighbours(
    persist: &PersistHandle,
    ctx: &LessonConsolidationCtx,
    lesson: &LessonRow,
    vec: &[f32],
    actions: &mut Vec<ConsolidationActionRecord>,
) -> Option<Vec<SimilarLesson>> {
    match persist
        .lessons()
        .find_similar_lessons(ctx.project_id, vec, lesson.agent_id, 3, lesson.id)
        .await
    {
        Ok(n) => Some(n),
        Err(e) => {
            log_find_neighbours_failed(lesson.id, ctx.project_id, &e);
            actions.push(skipped_action_record(
                lesson,
                ctx,
                format!("find_similar_lessons failed: {e}"),
            ));
            None
        }
    }
}

fn log_find_neighbours_failed(lesson_id: i64, project_id: i64, e: &PersistError) {
    tracing::warn!(
        error = %e,
        lesson_id = %lesson_id,
        project_id,
        "lesson_worker: find_similar_lessons failed — skipping lesson"
    );
}

fn log_near_duplicate_if_close(
    lesson: &LessonRow,
    nearest: Option<&SimilarLesson>,
    distance_threshold: f32,
) {
    if let Some(n) = nearest
        && n.distance < distance_threshold
    {
        tracing::info!(
            lesson_id = %lesson.id,
            nearest_id = %n.id,
            distance = %n.distance,
            threshold = %distance_threshold,
            "lesson_worker: near-duplicate lesson detected — promoting to daily anyway"
        );
    }
}

async fn record_actions_if_run_open(
    persist: &PersistHandle,
    run_id: Option<i64>,
    project_id: i64,
    actions: &[ConsolidationActionRecord],
) {
    let Some(run_id) = run_id else {
        return;
    };
    if let Err(e) = persist
        .lessons()
        .record_consolidation_actions(run_id, actions)
        .await
    {
        log_record_actions_failed(project_id, run_id, &e);
    }
}

fn log_record_actions_failed(project_id: i64, run_id: i64, e: &PersistError) {
    tracing::warn!(
        error = %e,
        project_id,
        run_id,
        "lesson_worker: failed to record consolidation actions — consolidation itself is unaffected"
    );
}
