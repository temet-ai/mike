use serde_json::{Value, json};

use super::OUTSIDE_PROJECT_SCORE_MARGIN;

pub fn is_outside_project(row: &Value) -> bool {
    row["provenance"]["outside_project"].as_bool() == Some(true)
}

pub fn mark_outside_project(row: &mut Value) {
    match row.get_mut("provenance").and_then(Value::as_object_mut) {
        Some(provenance) => {
            provenance.insert("outside_project".to_owned(), json!(true));
        }
        None => row["provenance"] = json!({"outside_project": true}),
    }
}

fn ordering_score((row, score): &(Value, Option<f32>)) -> f64 {
    let margin = if is_outside_project(row) {
        OUTSIDE_PROJECT_SCORE_MARGIN
    } else {
        0.0
    };
    f64::from(score.unwrap_or_default()) - margin
}

pub fn weigh_outside_project(
    arranged: Vec<(Value, Option<f32>)>,
) -> Vec<(Value, Option<f32>)> {
    if !arranged.iter().any(|(row, _)| is_outside_project(row)) {
        return arranged;
    }
    let (mut scored, unscored): (Vec<_>, Vec<_>) =
        arranged.into_iter().partition(|(_, score)| score.is_some());
    scored.sort_by(|a, b| ordering_score(b).total_cmp(&ordering_score(a)));
    scored.extend(unscored);
    scored
}

#[cfg(test)]
#[path = "outside_project_tests.rs"]
mod tests;
