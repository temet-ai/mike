use crate::store::persistence::lessons_repo::LessonsPersist as _;
use serde::Serialize;

use super::maintenance::{self, Reservation};
use super::placement::{self, LessonToPlace, PlacementAnswer};
use super::{ProposerDeps, guidance};
use crate::llm::LlmError;
use crate::store::persistence::lessons::maintenance_budget::{MaintenancePass, RefusalReason};
use crate::store::persistence::lessons::placement::{PlacementWrite, UnplacedLesson};
use crate::store::persistence::project_knowledge::GuidanceRow;

pub const PLACEMENTS_PER_TICK: i64 = 5;
const CONSECUTIVE_FAILURES_BEFORE_STOP: u32 = 2;

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct PlacementReport {
    pub placed: u64,
    pub failed: u64,
    pub budget_exhausted: bool,
}

enum Handled {
    Placed,
    Failed,
    Stop,
}

fn log_placement_failed(lesson_id: i64, message: &str) {
    tracing::warn!(
        lesson_id,
        error = message,
        "lesson_proposer: placement failed"
    );
}

fn log_adjustments(lesson_id: i64, notes: &[String]) {
    for note in notes {
        tracing::warn!(lesson_id, note, "lesson_proposer: placement adjusted");
    }
}

pub async fn run_pass(deps: &ProposerDeps<'_>) -> PlacementReport {
    let mut report = PlacementReport::default();
    let work = match deps
        .persist
        .lessons()
        .unplaced_lessons(PLACEMENTS_PER_TICK)
        .await
    {
        Ok(work) => work,
        Err(error) => {
            tracing::warn!(%error, "lesson_proposer: placement pass could not read unplaced lessons");
            return report;
        }
    };
    if work.is_empty() {
        return report;
    }
    let mut failures_in_a_row = 0;
    for lesson in &work {
        match place_one(deps, lesson, &mut report).await {
            Handled::Placed => failures_in_a_row = 0,
            Handled::Failed => failures_in_a_row += 1,
            Handled::Stop => break,
        }
        if failures_in_a_row >= CONSECUTIVE_FAILURES_BEFORE_STOP {
            break;
        }
    }
    report
}

async fn ask(
    deps: &ProposerDeps<'_>,
    lesson: &UnplacedLesson,
    shown: &[GuidanceRow],
) -> Result<PlacementAnswer, LlmError> {
    let subject = LessonToPlace {
        text: &lesson.text,
        kind: &lesson.kind,
        topic: lesson.topic.as_deref(),
        agent_name: lesson.agent_name.as_deref(),
    };
    placement::call(deps.llm, &subject, shown)
        .await
        .map(|(answer, _usage)| answer)
}

async fn place_one(
    deps: &ProposerDeps<'_>,
    lesson: &UnplacedLesson,
    report: &mut PlacementReport,
) -> Handled {
    match maintenance::reserve(deps, MaintenancePass::Placement).await {
        Reservation::Reserved => {}
        Reservation::Refused => {
            report.budget_exhausted = true;
            return Handled::Stop;
        }
        Reservation::Failed => return Handled::Stop,
    }
    let shown = guidance::closest_to_text(deps, lesson.project_id, &lesson.text).await;
    match ask(deps, lesson, &shown).await {
        Ok(answer) => store(deps, lesson, &answer, &shown, report).await,
        Err(error) if error.is_refusal() => {
            maintenance::note_refusal(
                deps,
                MaintenancePass::Placement,
                RefusalReason::ProviderLimit,
            )
            .await;
            report.budget_exhausted = true;
            Handled::Stop
        }
        Err(error) => failed(deps, report, lesson.id, &error.to_string()).await,
    }
}

async fn store(
    deps: &ProposerDeps<'_>,
    lesson: &UnplacedLesson,
    answer: &PlacementAnswer,
    shown: &[GuidanceRow],
    report: &mut PlacementReport,
) -> Handled {
    let mut notes = Vec::new();
    let resolved = placement::resolve(Some(answer), shown, &mut notes);
    log_adjustments(lesson.id, &notes);
    let write = PlacementWrite {
        home: resolved.home.code(),
        covered_by: resolved.covered_by.as_deref(),
        reason: &resolved.reason,
    };
    match deps
        .persist
        .lessons()
        .set_lesson_placement(lesson.id, write)
        .await
    {
        Ok(_) => {
            report.placed += 1;
            Handled::Placed
        }
        Err(error) => failed(deps, report, lesson.id, &error.to_string()).await,
    }
}

async fn failed(
    deps: &ProposerDeps<'_>,
    report: &mut PlacementReport,
    lesson_id: i64,
    message: &str,
) -> Handled {
    log_placement_failed(lesson_id, message);
    report.failed += 1;
    if let Err(error) = deps
        .persist
        .lessons()
        .record_placement_failure(lesson_id, message)
        .await
    {
        tracing::warn!(%error, lesson_id, "lesson_proposer: placement failure could not be recorded");
    }
    Handled::Failed
}
