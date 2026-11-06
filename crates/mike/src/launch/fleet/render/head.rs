use super::clock::{day_and_time, elapsed};
use super::ink::{
    DIM, FAINT, GREEN, INK, ORANGE, PURPLE, RED, YELLOW, badge, fit, fleet_state_colour, pad,
    paint, visible_len,
};
use super::tasks::is_passed;
use crate::launch::fleet::status::{CriterionStatus, FleetStatus};

const MISSING_ITEM_WIDTH: usize = 40;
const NOT_STARTED: [&str; 3] = ["draft", "ready", "failed"];

pub(super) fn missing(status: &FleetStatus) -> Vec<String> {
    if status.missing.is_empty() {
        return Vec::new();
    }
    let widest = status
        .missing
        .iter()
        .map(|m| visible_len(&m.item))
        .max()
        .unwrap_or(0)
        .min(MISSING_ITEM_WIDTH);
    let mut lines = vec![format!(
        " {}{}",
        paint("MISSING", RED, true),
        paint(
            &format!("   {}", missing_means(&status.state)),
            FAINT,
            false
        )
    )];
    for entry in &status.missing {
        lines.push(format!(
            "  {}  {}",
            pad(&paint(&entry.item, ORANGE, true), widest),
            paint(&entry.problem, INK, false)
        ));
    }
    lines
}

pub(super) fn missing_means(state: &str) -> &'static str {
    if NOT_STARTED.contains(&state) {
        "the fleet cannot start until each of these is fixed"
    } else {
        "the definition was changed after the fleet started and now has these problems; seats already open keep the task they were given"
    }
}

pub(super) fn header(status: &FleetStatus, width: usize) -> Vec<String> {
    let mut lines = vec![title(status, width)];
    if !status.goal.trim().is_empty() {
        lines.push(goal(&status.goal, width));
    }
    lines.push(summary(status));
    if let Some(broken) = &status.tree_broken {
        let text = crate::launch::fleet::shared_tree::line(broken, status.now);
        lines.push(fit(&format!(" {}", paint(&text, RED, true)), width));
    }
    if !status.criteria.is_empty() {
        lines.push(criteria(&status.criteria));
    }
    lines
}

fn goal(goal: &str, width: usize) -> String {
    let flat = goal.split_whitespace().collect::<Vec<_>>().join(" ");
    fit(&format!(" {}", paint(&flat, INK, false)), width)
}

fn title(status: &FleetStatus, width: usize) -> String {
    let left = format!(
        "{} {}  {}",
        badge(" FLEET ", PURPLE),
        paint(&status.slug, INK, true),
        paint(&status.state, fleet_state_colour(&status.state), false)
    );
    let clock = paint(&day_and_time(status.now), DIM, false);
    let room = width.saturating_sub(visible_len(&clock) + 1);
    format!("{}{clock}", pad(&left, room))
}

fn summary(status: &FleetStatus) -> String {
    let running = match (status.started_at, status.finished_at) {
        (Some(at), Some(end)) => format!("ran for {}", elapsed(at, end)),
        (Some(at), None) => format!("running for {}", elapsed(at, status.now)),
        (None, _) => "not started".to_string(),
    };
    let judged = status.tasks.iter().filter(|t| is_passed(&t.state)).count();
    let passes: usize = status.tasks.iter().map(|t| t.passes).sum();
    let fails: usize = status.tasks.iter().map(|t| t.fails).sum();
    let rounds = rounds_per_task(status);
    let open = status.seats.iter().filter(|s| !s.closed).count();
    let dot = paint("  ·  ", FAINT, false);
    let parts = [
        paint(&running, INK, false),
        paint(
            &format!(
                "{judged} of {} tasks accepted; {} ready; fleet acceptance {}",
                status.tasks.len(),
                status.tasks.iter().filter(|task| task.ready).count(),
                if status.accepted {
                    "accepted"
                } else {
                    "pending"
                }
            ),
            GREEN,
            false,
        ),
        format!(
            "{} {}{rounds}",
            paint(&format!("{passes} pass"), GREEN, false),
            paint(&format!("{fails} fail"), RED, false)
        ),
        paint(
            &format!("{open} seats open, {} launched", status.seats.len()),
            INK,
            false,
        ),
    ];
    format!(" {}", parts.join(&dot))
}

fn rounds_per_task(status: &FleetStatus) -> String {
    let judged: Vec<usize> = status
        .tasks
        .iter()
        .map(|t| t.rounds)
        .filter(|rounds| *rounds > 0)
        .collect();
    if judged.is_empty() {
        return String::new();
    }
    let per_task = judged.iter().sum::<usize>() as f64 / judged.len() as f64;
    let colour = if per_task >= 2.0 { RED } else { INK };
    paint(&format!(" {per_task:.1} rounds/task"), colour, false)
}

fn criteria(criteria: &[CriterionStatus]) -> String {
    let cells: Vec<String> = criteria
        .iter()
        .map(|c| {
            let colour = if c.tasks == 0 || c.passed == 0 {
                DIM
            } else if c.passed >= c.tasks {
                GREEN
            } else {
                YELLOW
            };
            paint(
                &format!(
                    "{} {}/{} accepted, {} ready",
                    c.id, c.passed, c.tasks, c.ready
                ),
                colour,
                false,
            )
        })
        .collect();
    format!(" {}{}", paint("criteria  ", DIM, false), cells.join("  "))
}

#[cfg(test)]
mod tests {
    use super::missing_means;

    #[test]
    fn missing_items_under_a_running_fleet_are_not_called_a_bar_to_starting() {
        for state in ["draft", "ready", "failed"] {
            assert_eq!(
                missing_means(state),
                "the fleet cannot start until each of these is fixed"
            );
        }
        for state in ["starting", "running", "done"] {
            assert!(
                missing_means(state)
                    .starts_with("the definition was changed after the fleet started"),
                "{state}"
            );
        }
    }
}
