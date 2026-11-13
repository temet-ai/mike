use super::clock::hour_and_minute;
use super::ink::{FAINT, INK, ORANGE, RED, paint};
use crate::launch::fleet::status::FleetStatus;

pub fn attention(status: &FleetStatus) -> Vec<String> {
    let mut lines = Vec::new();
    for obligation in status
        .obligations
        .iter()
        .filter(|obligation| obligation.required && !obligation.accepted())
    {
        lines.push(format!(
            " {}  {}: {} — {}",
            paint("ACCEPTANCE OPEN", ORANGE, true),
            obligation.obligation_id,
            obligation.outcome,
            obligation.title
        ));
        if let Some(issue) = &obligation.issue {
            lines.push(format!("  {}", paint(issue, RED, false)));
        }
    }
    if !status.needs_operator.is_empty() {
        lines.push(format!(" {}", paint("NEEDS YOU", RED, true)));
    }
    lines.extend(status.needs_operator.iter().map(|entry| {
        format!(
            "  {}  {}  {}",
            paint(&hour_and_minute(entry.at), FAINT, false),
            paint(&entry.seat, ORANGE, false),
            paint(&entry.title, INK, false)
        )
    }));
    if let Some(seconds) = status.every_seat_idle_seconds {
        lines.push(format!(
            " {}  {}",
            paint("NOTHING IS MOVING", ORANGE, true),
            paint(
                &format!("every open seat has been idle for {}m", seconds / 60),
                INK,
                false
            )
        ));
    }
    lines.extend(status.problems.iter().map(problem_line));
    lines
}

fn problem_line(problem: &crate::launch::fleet::status::ProblemStatus) -> String {
    let standing = match (problem.escalated, problem.decisions.last()) {
        (true, _) => paint("escalated to you", RED, false),
        (false, Some(decision)) => paint(&format!("decided: {decision}"), INK, false),
        (false, None) => paint("waiting for the problem-solver", FAINT, false),
    };
    format!(
        " {}  {}  {}{}  {}  {}",
        paint("PROBLEM", ORANGE, true),
        paint(&hour_and_minute(problem.opened_at), FAINT, false),
        paint(&problem.trigger, INK, true),
        problem
            .task
            .as_deref()
            .map(|task| paint(&format!(" {task}"), INK, false))
            .unwrap_or_default(),
        standing,
        paint(&problem.detail, INK, false)
    )
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::attention;
    use crate::launch::fleet::render::strip;
    use crate::launch::fleet::status::{FleetStatus, NeedsOperator};

    fn status(now: DateTime<Utc>) -> FleetStatus {
        FleetStatus {
            fleet: "f".to_string(),
            slug: "night-run".to_string(),
            state: "running".to_string(),
            run_id: None,
            goal: "ship it".to_string(),
            started_at: None,
            finished_at: None,
            now,
            missing: Vec::new(),
            criteria: Vec::new(),
            tasks: Vec::new(),
            seats: Vec::new(),
            verdicts: Vec::new(),
            blocked: Vec::new(),
            needs_operator: Vec::new(),
            accepted: false,
            obligations: vec![],
            every_seat_idle_seconds: None,
            tree_broken: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn a_fleet_that_needs_nobody_and_is_moving_says_nothing() -> anyhow::Result<()> {
        let now: DateTime<Utc> = "2026-10-01T16:30:00Z".parse()?;
        assert!(attention(&status(now)).is_empty());
        Ok(())
    }

    #[test]
    fn what_only_the_operator_can_settle_and_a_fleet_at_a_standstill_are_said_first()
    -> anyhow::Result<()> {
        let now: DateTime<Utc> = "2026-10-01T16:30:00Z".parse()?;
        let mut waiting = status(now);
        waiting.needs_operator.push(NeedsOperator {
            at: "2026-10-01T15:59:00Z".parse()?,
            seat: "coordinator".to_string(),
            title: "OPERATOR: commits are refused in a cycle".to_string(),
        });
        waiting.every_seat_idle_seconds = Some(1_620);
        let lines: Vec<String> = attention(&waiting).iter().map(|l| strip(l)).collect();
        assert_eq!(
            lines,
            vec![
                " NEEDS YOU".to_string(),
                "  15:59  coordinator  OPERATOR: commits are refused in a cycle".to_string(),
                " NOTHING IS MOVING  every open seat has been idle for 27m".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_finished_fleet_says_how_long_it_ran_and_not_that_it_is_running() -> anyhow::Result<()> {
        let now: DateTime<Utc> = "2026-10-01T19:00:00Z".parse()?;
        let mut done = status(now);
        done.state = "done".to_string();
        done.started_at = Some("2026-10-01T17:59:00Z".parse()?);
        done.finished_at = Some("2026-10-01T18:26:00Z".parse()?);
        let text = strip(&crate::launch::fleet::render::render(&done, 120));
        assert!(text.contains("ran for 0h27m"), "{text}");
        assert!(!text.contains("running for"), "{text}");
        Ok(())
    }
}
