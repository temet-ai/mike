use crate::store::persistence::web_repo::WebPersist as _;

use super::context::EngineerContext;
use super::finding::Validated;
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::NewFollowup;

pub(super) struct Origin {
    pub(super) finding_id: i64,
    pub(super) problem_id: Option<i64>,
    pub(super) followup_id: Option<i64>,
}

async fn check_name(ctx: &EngineerContext, problem_id: Option<i64>) -> Option<String> {
    let id = problem_id?;
    match ctx.persist.web().wright_problem(id).await {
        Ok(problem) => problem.map(|p| p.check_name),
        Err(e) => {
            tracing::warn!(error = %e, problem = id, "wright: the problem's check could not be read; its follow-ups are recorded without one");
            None
        }
    }
}

async fn root_cause_title(ctx: &EngineerContext, check: Option<&str>, followup_id: i64) -> String {
    let found = match check {
        Some(check) => ctx
            .persist
            .web()
            .live_wright_followups(Some(check), None)
            .await
            .ok()
            .and_then(|rows| rows.into_iter().find(|row| row.id == followup_id)),
        None => None,
    };
    found.map_or_else(
        || format!("Find why the check keeps failing (follow-up {followup_id})"),
        |row| row.title,
    )
}

fn log_recorded(result: Result<i64, PersistError>, finding_id: i64, title: &str) {
    match result {
        Ok(id) => tracing::debug!(
            followup = id,
            finding = finding_id,
            "wright: follow-up recorded"
        ),
        Err(e) => {
            tracing::warn!(error = %e, finding = finding_id, %title, "wright: a follow-up could not be recorded")
        }
    }
}

pub(super) async fn record(ctx: &EngineerContext, origin: &Origin, validated: &Validated) {
    let check = check_name(ctx, origin.problem_id).await;
    for draft in &validated.followups {
        let new = NewFollowup {
            kind: &draft.kind,
            check_name: check.as_deref(),
            owning_part: draft.owning_part.as_deref(),
            title: &draft.title,
            body: &draft.body,
            finding_id: origin.finding_id,
            same_as: draft.same_as,
        };
        let result = ctx.persist.web().record_wright_followup(&new).await;
        log_recorded(result, origin.finding_id, &draft.title);
    }
    if let Some(followup_id) = origin.followup_id {
        link_root_cause(ctx, origin, check.as_deref(), validated, followup_id).await;
    }
}

async fn link_root_cause(
    ctx: &EngineerContext,
    origin: &Origin,
    check: Option<&str>,
    validated: &Validated,
    followup_id: i64,
) {
    let finding = &validated.finding;
    let title = root_cause_title(ctx, check, followup_id).await;
    let body = finding
        .confirmed_cause
        .as_deref()
        .unwrap_or(&finding.suspected_cause);
    let new = NewFollowup {
        kind: "root_cause",
        check_name: check,
        owning_part: finding.owning_part.as_deref(),
        title: &title,
        body,
        finding_id: origin.finding_id,
        same_as: Some(followup_id),
    };
    let result = ctx.persist.web().record_wright_followup(&new).await;
    log_recorded(result, origin.finding_id, &title);
}
