use clap::Args;
use serde_json::json;
use uuid::Uuid;

use super::api::{Api, FleetError};
use super::{LIST_LIMIT, ListAnswer};
use crate::launch::fleet::commit::{CommitReport, IntegrateReport};
use crate::launch::fleet::push::{
    PUSH_ATTEMPTS, PUSH_RETRY, behind_origin, fetch_origin, push_once, will_not_pass_on_a_retry,
};

const COMMIT_WAIT: std::time::Duration = std::time::Duration::from_secs(600);

#[derive(Debug, Clone, Args)]
pub struct CommitArgs {
    #[arg(
        short = 'm',
        long,
        help = "The commit message; the seat and its session are appended as a trailer"
    )]
    pub message: String,
    #[arg(
        long,
        help = "The fleet's uuid, slug or run id; default: the MIKE_RUN_ID of this pane"
    )]
    pub fleet: Option<String>,
    #[arg(
        long,
        help = "The seat's session id; default: the MIKE_SESSION_ID of this pane"
    )]
    pub session: Option<Uuid>,
    #[arg(
        long = "path",
        help = "A path to commit; repeatable. Default: every changed file the seat's task owns"
    )]
    pub paths: Vec<String>,
    #[arg(
        long,
        help = "Write the commit even when no file changed, for a message that must land, such as a closing `Closes #n`"
    )]
    pub allow_empty: bool,
}

pub(super) fn own_session(given: Option<Uuid>) -> Result<Uuid, FleetError> {
    match given {
        Some(session) => Ok(session),
        None => std::env::var("MIKE_SESSION_ID")
            .ok()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                FleetError::Refused(
                    "no session: pass --session, or run this inside a pane mike launched".into(),
                )
            }),
    }
}

pub(super) fn own_fleet(given: Option<&str>) -> Result<String, FleetError> {
    match given {
        Some(key) => Ok(key.to_string()),
        None => std::env::var("MIKE_RUN_ID").ok().ok_or_else(|| {
            FleetError::Refused(
                "no fleet: pass --fleet, or run this inside a pane of a fleet mike launched".into(),
            )
        }),
    }
}

pub(super) async fn commit_text(api: &Api, args: &CommitArgs) -> Result<String, FleetError> {
    let session = own_session(args.session)?;
    let key = own_fleet(args.fleet.as_deref())?;
    let fleet = resolve(api, &key).await?;
    let body = json!({
        "session": session,
        "message": args.message,
        "paths": args.paths,
        "allow_empty": args.allow_empty
    });
    let answer = api
        .post_json_and_wait(api.url(&[&fleet, "commit"])?, &body, COMMIT_WAIT)
        .await?;
    let mut report: CommitReport = api.parse(&answer)?;
    push_from_this_shell(api, &fleet, &mut report).await;
    Ok(render(&report))
}

pub(super) async fn resolve(api: &Api, key: &str) -> Result<String, FleetError> {
    if !key.starts_with("fleet-") {
        return Ok(key.to_string());
    }
    let mut url = api.url(&[])?;
    url.query_pairs_mut().append_pair("limit", LIST_LIMIT);
    let body = api.get(url).await?;
    let answer: ListAnswer = api.parse(&body)?;
    answer
        .fleets
        .iter()
        .find(|f| f.run_id.as_deref() == Some(key))
        .map(|f| f.fleet.clone())
        .ok_or_else(|| FleetError::Refused(format!("no fleet has the run id `{key}`")))
}

async fn push_from_this_shell(api: &Api, fleet: &str, report: &mut CommitReport) {
    if report.sha.is_none() || report.pushed || report.repo.is_empty() {
        return;
    }
    match push_until_it_lands(api, fleet, &report.repo, &report.branch).await {
        Ok(()) => {
            report.pushed = true;
            report.push_error = None;
        }
        Err(why) => report.push_error = Some(why),
    }
}

async fn push_until_it_lands(
    api: &Api,
    fleet: &str,
    repo: &str,
    branch: &str,
) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..PUSH_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(PUSH_RETRY).await;
        }
        match push_once(repo, branch).await {
            Ok(()) => return Ok(()),
            Err(why) if will_not_pass_on_a_retry(&why) => return Err(why),
            Err(why) => {
                if behind_origin(&why) {
                    bring_in_origin(api, fleet, repo, branch).await?;
                }
                last = why;
            }
        }
    }
    Err(last)
}

async fn bring_in_origin(api: &Api, fleet: &str, repo: &str, branch: &str) -> Result<(), String> {
    fetch_origin(repo, branch).await?;
    let url = api.url(&[fleet, "integrate"]).map_err(|e| e.to_string())?;
    let answer = api.post(url).await.map_err(|e| e.to_string())?;
    let merged: IntegrateReport = api.parse(&answer).map_err(|e| e.to_string())?;
    merged.problem.map_or(Ok(()), Err)
}

fn warnings_text(report: &CommitReport) -> String {
    report
        .warnings
        .iter()
        .map(|w| format!("\nwarning: {w}"))
        .collect()
}

fn push_text(report: &CommitReport) -> String {
    match (report.pushed, &report.push_error) {
        (true, _) => ", pushed".to_string(),
        (false, Some(why)) => format!(", not pushed ({why})"),
        (false, None) => ", not pushed".to_string(),
    }
}

pub(super) fn render(report: &CommitReport) -> String {
    match (&report.refused, &report.sha) {
        (Some(why), _) => format!("nothing committed: {why}"),
        (None, Some(sha)) => format!(
            "{} committed {} as {}{}:\n{}\nverified: {}{}",
            report.seat,
            report.task,
            &sha[..sha.len().min(10)],
            push_text(report),
            report
                .paths
                .iter()
                .map(|p| format!("  {p}"))
                .collect::<Vec<_>>()
                .join("\n"),
            report.verified,
            warnings_text(report)
        ),
        (None, None) => "nothing committed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_reads_as_one_paragraph() {
        let report = CommitReport {
            seat: "db-dev-T1".into(),
            task: "T1".into(),
            paths: vec!["a.rs".into()],
            sha: Some("0123456789abcdef".into()),
            pushed: true,
            verified: "check passed at tree state abc with exactly these files uncommitted".into(),
            ..CommitReport::default()
        };
        assert_eq!(
            render(&report),
            "db-dev-T1 committed T1 as 0123456789, pushed:\n  a.rs\nverified: check passed at tree state abc with exactly these files uncommitted"
        );
        let refused = CommitReport {
            refused: Some("`x` has not changed".into()),
            sha: None,
            ..report.clone()
        };
        assert_eq!(render(&refused), "nothing committed: `x` has not changed");
        let warned = CommitReport {
            pushed: false,
            push_error: Some("fatal: could not read Username".into()),
            warnings: vec!["`b.rs` is not one of task T1's files and not a shared file".into()],
            ..report
        };
        assert!(render(&warned).contains("not pushed (fatal: could not read Username)"));
        assert!(
            render(&warned)
                .ends_with("\nwarning: `b.rs` is not one of task T1's files and not a shared file")
        );
    }
}
