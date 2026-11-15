use std::collections::BTreeMap;

use crate::lesson_worker::LessonRoute;

use super::model::{LessonRow, STANDING_CAP};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteDecision {
    pub lesson_id: i64,
    pub project_id: i64,
    pub route: LessonRoute,
    pub covered_by: Option<String>,
    pub revision: i32,
}

fn has_topic(row: &LessonRow) -> bool {
    row.topic
        .as_deref()
        .is_some_and(|topic| !topic.trim().is_empty())
}

fn is_workflow_topic(row: &LessonRow) -> bool {
    row.topic
        .as_deref()
        .is_some_and(|topic| topic.starts_with("shared:") || topic.starts_with("verifier:"))
}

fn belongs_in_standing_set(row: &LessonRow) -> bool {
    let placed_for_memory = match row.placement.as_deref() {
        Some("memory") => true,
        None => !has_topic(row),
        Some(_) => false,
    };
    row.agent_id.is_none()
        && !is_workflow_topic(row)
        && placed_for_memory
        && (row.stage == "long_term" || row.source == "operator")
}

pub(super) fn decide_routes(
    lessons: &[LessonRow],
    covered: &BTreeMap<i64, String>,
) -> Vec<RouteDecision> {
    let mut standing: Vec<&LessonRow> = lessons
        .iter()
        .filter(|row| !covered.contains_key(&row.id) && belongs_in_standing_set(row))
        .collect();
    standing.sort_by_key(|row| std::cmp::Reverse(row.id));
    standing.truncate(STANDING_CAP);
    let chosen: Vec<i64> = standing.iter().map(|row| row.id).collect();
    let mut decisions: Vec<RouteDecision> = lessons
        .iter()
        .map(|row| {
            let covered_by = covered.get(&row.id).cloned();
            let route = if covered_by.is_some() {
                LessonRoute::Covered
            } else if chosen.contains(&row.id) {
                LessonRoute::Standing
            } else {
                LessonRoute::Injection
            };
            RouteDecision {
                lesson_id: row.id,
                project_id: row.project_id,
                route,
                covered_by,
                revision: row.revision,
            }
        })
        .collect();
    decisions.sort_by_key(|decision| decision.lesson_id);
    decisions
}

pub(super) fn standing_rows<'a>(
    lessons: &'a [LessonRow],
    decisions: &[RouteDecision],
) -> Vec<&'a LessonRow> {
    let mut rows: Vec<&LessonRow> = lessons
        .iter()
        .filter(|row| {
            decisions.iter().any(|decision| {
                decision.lesson_id == row.id && decision.route == LessonRoute::Standing
            })
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.id));
    rows
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod routes_tests;
