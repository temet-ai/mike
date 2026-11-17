use crate::store::persistence::web_repo::WebPersist as _;

use super::FollowupError;
use super::context::FollowupContext;
use super::file::{FileReport, file};
use super::follow::{FollowReport, follow};
use super::handoff::{Handoff, hand_off};
use super::recurring::comment_recurrences;
use super::runner::CommandRunner;
use super::sessions::FixerSessions;
use crate::store::persistence::wright::FollowupLimits;

#[derive(Debug, Default)]
pub(crate) struct TickReport {
    pub(crate) root_causes: Vec<i64>,
    pub(crate) filed: Option<FileReport>,
    pub(crate) handoff: Option<Handoff>,
    pub(crate) followed: Option<FollowReport>,
}

pub(crate) async fn recurrence<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<Vec<i64>, FollowupError> {
    let web = ctx.persist.web();
    let days = limits.recurrence_days;
    let mut opened = Vec::new();
    for row in web
        .recurring_wright_remedies(days, i64::from(limits.recurrence_actions))
        .await?
    {
        if let Some(id) = web
            .open_root_cause_followup(&row.check_name, &row.remedy, row.actions, days)
            .await?
        {
            opened.push(id);
        }
    }
    Ok(opened)
}

fn failed(step: &str, error: &FollowupError) {
    tracing::warn!(step, error = %error, "wright follow-ups: a step failed");
}

fn log_rows(step: &str, failures: &[(i64, String)]) {
    for (id, error) in failures {
        tracing::warn!(step, followup = id, error = %error, "wright follow-ups: one follow-up failed");
    }
}

pub(crate) async fn tick<R, S>(ctx: &FollowupContext<R, S>, limits: &FollowupLimits) -> TickReport
where
    R: CommandRunner,
    S: FixerSessions,
{
    let mut report = TickReport::default();
    match recurrence(ctx, limits).await {
        Ok(opened) => report.root_causes = opened,
        Err(e) => failed("recurrence", &e),
    }
    match file(ctx, limits).await {
        Ok(filed) => {
            log_rows("file", &filed.failed);
            report.filed = Some(filed);
        }
        Err(e) => failed("file", &e),
    }
    match hand_off(ctx, limits).await {
        Ok(handoff) => report.handoff = Some(handoff),
        Err(e) => failed("hand off", &e),
    }
    match follow(ctx, limits).await {
        Ok(followed) => {
            log_rows("follow", &followed.failed);
            report.followed = Some(followed);
        }
        Err(e) => failed("follow", &e),
    }
    match comment_recurrences(ctx, limits).await {
        Ok(failures) => log_rows("comment recurrence", &failures),
        Err(e) => failed("comment recurrence", &e),
    }
    report
}

pub(crate) fn log_tick(report: &TickReport) {
    let filed = report.filed.as_ref().map_or(&[][..], |f| &f.filed[..]);
    let followed = report
        .followed
        .as_ref()
        .map_or(&[][..], |f| &f.followed[..]);
    let moved = followed
        .iter()
        .any(|(_, step)| !matches!(step, super::follow::Followed::Working));
    let handed = matches!(report.handoff, Some(Handoff::HandedOff { .. }));
    if !report.root_causes.is_empty() || !filed.is_empty() || handed || moved {
        log_changes(report, filed, followed);
    }
    if let Some(Handoff::NoQuietMachine(reasons)) = &report.handoff {
        log_no_quiet_machine(reasons);
    }
}

fn log_changes<F: std::fmt::Debug, W: std::fmt::Debug>(
    report: &TickReport,
    filed: &[F],
    followed: &[W],
) {
    tracing::info!(
        root_causes = ?report.root_causes,
        filed = ?filed,
        handoff = ?report.handoff,
        followed = ?followed,
        "wright follow-ups: tick"
    );
}

fn log_no_quiet_machine<R: std::fmt::Debug>(reasons: &R) {
    tracing::debug!(?reasons, "wright follow-ups: no fixer machine is quiet");
}
