use std::process::Stdio;

use serde::Deserialize;

use super::command::{Api, FleetError, FocusArgs};
use super::status::FleetStatus;

#[derive(Deserialize)]
struct FleetHead {
    run_id: Option<String>,
}

#[derive(Deserialize)]
struct LaunchSeat {
    #[serde(default)]
    backend: Option<String>,
    seat: Option<String>,
    surface_ref: Option<String>,
    workspace_ref: Option<String>,
    closed_at: Option<String>,
}

pub async fn run(api: &Api, args: &FocusArgs) -> Result<String, FleetError> {
    let head: FleetHead = api.parse(&api.get(api.url(&[&args.fleet])?).await?)?;
    let run = head.run_id.ok_or_else(|| {
        FleetError::Refused("the fleet has not started, so it has no seats".into())
    })?;
    let seat = match &args.seat {
        Some(seat) => seat.clone(),
        None => {
            let status: FleetStatus =
                api.parse(&api.get(api.url(&[&args.fleet, "status"])?).await?)?;
            seat_to_show(&status).ok_or_else(|| {
                FleetError::Refused("no seat is blocked and nothing has failed; name a seat".into())
            })?
        }
    };
    let mut url = api.url_at("/api/launches")?;
    url.query_pairs_mut().append_pair("run", &run);
    let launches: Vec<LaunchSeat> = api.parse(&api.get(url).await?)?;
    let hosting = launches
        .iter()
        .filter(|l| l.closed_at.is_none())
        .find(|l| l.seat.as_deref() == Some(seat.as_str()))
        .ok_or_else(|| FleetError::Refused(format!("seat `{seat}` has no open pane")))?;
    let pane = hosting
        .surface_ref
        .clone()
        .ok_or_else(|| FleetError::Refused(format!("seat `{seat}` has no pane on record")))?;
    if hosting.backend.as_deref() == Some("herdr-core") {
        run_tool("herdr-app", &["do", "agent.focus", "--terminal", &pane]).await?;
        return Ok(format!(
            "{seat}: terminal {pane} is at the front of the Herdr app"
        ));
    }
    let Some(workspace) = &hosting.workspace_ref else {
        return Ok(format!("{seat} is in pane {pane}"));
    };
    if !args.switch && !workspace_in_front(workspace).await {
        return Ok(format!(
            "{seat} is in pane {pane} of workspace {workspace}, which is not in front; pass --switch to go there"
        ));
    }
    herdr(&["workspace", "focus", workspace]).await?;
    herdr(&["agent", "focus", &pane]).await?;
    Ok(format!("{seat}: pane {pane} is at the front"))
}

async fn workspace_in_front(workspace: &str) -> bool {
    let Ok(output) = tokio::process::Command::new("herdr")
        .args(["workspace", "list"])
        .stdin(Stdio::null())
        .output()
        .await
    else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return false;
    };
    value["result"]["workspaces"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|w| w["workspace_id"] == workspace && w["focused"] == true)
}

pub fn seat_to_show(status: &FleetStatus) -> Option<String> {
    if let Some(blocked) = status.blocked.first()
        && !blocked.seat.is_empty()
    {
        return Some(blocked.seat.clone());
    }
    let failed = status.verdicts.iter().find(|v| v.verdict == "fail")?;
    let task = failed.tasks.first()?;
    status
        .tasks
        .iter()
        .find(|t| &t.id == task)
        .and_then(|t| t.seat.clone())
}

async fn herdr(args: &[&str]) -> Result<(), FleetError> {
    run_tool("herdr", args).await
}

async fn run_tool(program: &str, args: &[&str]) -> Result<(), FleetError> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| FleetError::Refused(format!("could not run {program}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(FleetError::Refused(format!(
        "{program} {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
pub mod tests {
    use chrono::Utc;

    use super::seat_to_show;
    use crate::launch::fleet::status::{Blocked, FleetStatus, TaskStatus, Verdict};

    fn status() -> FleetStatus {
        FleetStatus {
            fleet: "f".into(),
            slug: "s".into(),
            state: "running".into(),
            run_id: None,
            goal: String::new(),
            started_at: None,
            finished_at: None,
            now: Utc::now(),
            missing: vec![],
            criteria: vec![],
            tasks: vec![TaskStatus {
                id: "T2".into(),
                name: "second".into(),
                discipline: "db-dev".into(),
                criteria: vec![],
                seat: Some("db-dev-T2".into()),
                last_seat: None,
                state: "rework".into(),
                passes: 0,
                fails: 1,
                rounds: 0,
                last: None,
                passed_criteria: vec![],
                ready: false,
            }],
            seats: vec![],
            verdicts: vec![Verdict {
                at: Utc::now(),
                verdict: "fail".into(),
                title: "FAIL".into(),
                tasks: vec!["T2".into()],
                criteria: vec![],
            }],
            blocked: vec![],
            needs_operator: Vec::new(),
            accepted: false,
            obligations: vec![],
            every_seat_idle_seconds: None,
            tree_broken: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn a_blocked_seat_comes_before_the_seat_of_a_failed_task() {
        let mut s = status();
        assert_eq!(seat_to_show(&s).as_deref(), Some("db-dev-T2"));
        s.blocked.push(Blocked {
            seat: "frontend-developer-T1".into(),
            title: "waiting".into(),
        });
        assert_eq!(seat_to_show(&s).as_deref(), Some("frontend-developer-T1"));
        s.blocked.clear();
        s.verdicts.clear();
        assert_eq!(seat_to_show(&s), None);
    }
}
