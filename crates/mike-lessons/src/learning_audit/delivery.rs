use serde_json::{Value, json};

use super::fetch::Window;
use super::rows::{int, percent, rows, text};
use crate::api::lesson_delivery_comparison::ArmReport;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

const ARMS: [&str; 2] = ["mod", "control"];

pub(super) async fn comparison(handle: &PersistHandle, window: &Window) -> anyhow::Result<Value> {
    let arms = handle
        .lessons()
        .compare_lesson_delivery_arms(window.start, window.end)
        .await?
        .into_iter()
        .map(|row| serde_json::to_value(ArmReport::from(row)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"from": window.start, "until": window.end, "arms": arms}))
}

fn figure(value: Option<f64>, digits: usize) -> String {
    value.map_or_else(
        || "no data".to_owned(),
        |number| format!("{number:.digits$}"),
    )
}

fn share(value: Option<f64>) -> String {
    value.map_or_else(|| "no data".to_owned(), percent)
}

fn arm_row(arm: &Value) -> String {
    let total =
        int(arm, "input_tokens") + int(arm, "cache_read_tokens") + int(arm, "cache_write_tokens");
    format!(
        "| {} | {} | {} | {} ({} lessons / {} turns) | {} ({} / {total} input tokens) | {} ({} corrected / {} judged) |",
        text(arm, "arm"),
        int(arm, "sessions"),
        int(arm, "operator_turns"),
        figure(arm["lessons_per_turn"].as_f64(), 2),
        int(arm, "lessons_delivered"),
        int(arm, "operator_turns"),
        share(arm["cache_read_share"].as_f64()),
        int(arm, "cache_read_tokens"),
        share(arm["corrected_rate"].as_f64()),
        int(arm, "corrected_turns"),
        int(arm, "judged_turns"),
    )
}

pub(super) fn render(comparison: &Value) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "## Lesson delivery: mike mod against proxy injection".to_owned(),
        String::new(),
    ];
    let arms = rows(comparison, "arms");
    let find = |name: &str| arms.iter().find(|arm| arm["arm"] == name);
    lines.push(
        concat!(
            "While the `mod_lesson_delivery` setting is on, one session in five stays on proxy injection (the control ",
            "arm) and the rest get their lessons from the mike mod as hook context (the mod arm). A session is placed ",
            "in an arm by a hash of its id, so the arms are comparable but not matched by project or task. ",
            "A rate over a few turns is a reason to read the sessions, not a result."
        )
        .to_owned(),
    );
    lines.push(String::new());
    if !ARMS
        .iter()
        .any(|name| find(name).is_some_and(|arm| int(arm, "sessions") != 0))
    {
        lines.push(
            "No session in either arm made a request in this window. The setting was off, or no session has the mike mod installed."
                .to_owned(),
        );
        return lines;
    }
    lines.push(
        "| Arm | Sessions | Operator turns | Lessons per turn | Cache read share of input tokens | Judge corrected rate |"
            .to_owned(),
    );
    lines.push("|---|---:|---:|---|---|---|".to_owned());
    lines.extend(ARMS.iter().filter_map(|name| find(name)).map(arm_row));
    lines.extend(ARMS.iter().filter_map(|name| find(name)).map(|arm| {
        format!(
            "- {}: {} of {} requests carried token usage.",
            text(arm, "arm"),
            int(arm, "usage_requests"),
            int(arm, "requests"),
        )
    }));
    lines
}
