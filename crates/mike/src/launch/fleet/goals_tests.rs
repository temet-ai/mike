#![allow(
    clippy::expect_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use super::*;
use crate::launch::fleet::definition::{Criterion, GoalsDefinition};

fn definition() -> FleetDefinition {
    let mut def = FleetDefinition {
        goals: GoalsDefinition {
            workers: "task {task} ({name}) is accepted: {criteria}".into(),
        },
        ..FleetDefinition::default()
    };
    def.criteria.insert(
        "C1".into(),
        Criterion {
            text: "the form sends mail and all pricing plans can be purchased".into(),
            check: "a message lands and a purchase completes".into(),
            ..Criterion::default()
        },
    );
    def.tasks.insert(
        "T1".into(),
        TaskDefinition {
            name: "Contact form".into(),
            discipline: "web".into(),
            brief: "Implement the contact form. Exercise: submit it and show the received mail."
                .into(),
            criteria: vec!["C1".into(), "C9".into()],
            session: Some("worker-session".into()),
            ..TaskDefinition::default()
        },
    );
    def.tasks.insert(
        "T2".into(),
        TaskDefinition {
            name: "Pricing".into(),
            discipline: "web".into(),
            goal: "the pricing page lists every plan".into(),
            ..TaskDefinition::default()
        },
    );
    def
}

fn goal(def: &FleetDefinition, seat: &str) -> String {
    seat_goal(def, seat).expect("configured worker has a goal")
}

fn task<'a>(def: &'a mut FleetDefinition, id: &str) -> &'a mut TaskDefinition {
    def.tasks.get_mut(id).expect("fixture task exists")
}

#[test]
fn control_seats_legacy_definition_have_no_goal() {
    let def: FleetDefinition = serde_json::from_value(serde_json::json!({"goals": {
        "fleet": "every criterion is signed off and the fleet is closed; or stop after 60 turns",
        "coordinator": "the fleet is closed",
        "acceptance": "every task has a final verdict",
        "observer": "the run is filed",
        "workers": "task {task} is accepted"
    }}))
    .expect("legacy goal fields are ignored");
    assert_eq!(def.goals.workers, "task {task} is accepted");
    for seat in ["coordinator", "acceptance", "observer"] {
        assert_eq!(seat_goal(&def, seat), None, "{seat}");
        assert_eq!(brief_line(&def, seat), "", "{seat}");
    }
}

#[test]
fn worker_goal_shared_criterion_uses_only_task_obligations() {
    let def = definition();
    let text = goal(&def, "web-T1");
    assert!(text.contains("task T1 (Contact form) is accepted: C1, C9: only Contact form"));
    assert!(text.contains("task T1's assigned contribution per its brief"));
    assert!(!text.contains("Exercise: submit"));
    assert!(!text.contains("pricing plans can be purchased"));
    assert!(!text.contains("a purchase completes"));
    assert!(text.contains("C9"));
}

#[test]
fn worker_goal_explicit_task_goal_preserves_text_and_scope() {
    let def = definition();
    let text = goal(&def, "web-T2");
    assert!(text.contains("Configured goal: the pricing page lists every plan"));
    assert!(!text.contains("is accepted:"));
    assert!(text.contains("Task T2, seat web-T2: only its assigned contribution per brief"));
    assert!(text.contains("`Fleet-Seat: web-T2 (`"));
}

#[test]
fn worker_goal_explicit_template_substitutes_task_local_placeholders() {
    let mut def = definition();
    task(&mut def, "T1").goal = "{task}: {name}; prove {criteria}".into();
    let text = goal(&def, "web-T1");
    assert!(text.contains("Configured goal: T1: Contact form; prove C1, C9: only Contact form"));
    assert!(!text.contains("{criteria}"));
    assert!(!text.contains("pricing plans can be purchased"));
}

#[test]
fn worker_goal_peer_commit_cannot_invalidate_own_evidence() {
    let def = definition();
    let text = goal(&def, "web-T1");
    assert!(text.contains("`Fleet-Seat: web-T1 (worker-session)`"));
    assert!(text.contains("judge files/checks at own SHA(s)"));
    assert!(text.contains("never shared HEAD/worktree or peers' commits"));
}

#[test]
fn worker_goal_external_acceptance_wait_is_satisfied_only_after_handoff() {
    let mut def = definition();
    task(&mut def, "T1").goal = "commit the flip after acceptance passes C1-C4".into();
    let text = goal(&def, "web-T1");
    assert!(text.contains("Configured goal: commit the flip after acceptance passes C1-C4"));
    assert!(text.contains("These rules override broader conditions"));
    assert!(text.contains("all actionable work is done"));
    assert!(text.contains("external blocker plus next action is recorded with ask (blocked true)"));
    assert!(text.contains("Stop polling"));
    assert!(text.contains("resume on actionable mail/digest"));
    assert!(text.contains("Waiting is not acceptance or task completion"));
}

#[test]
fn worker_goal_completed_handoff_requires_task_local_work_and_evidence() {
    let def = definition();
    let text = goal(&def, "web-T1");
    assert!(text.contains("Goal met after all own work, exercise/evidence and fleet_report"));
    assert!(text.contains("acceptance/gates/peers need not finish"));
}

#[test]
fn task_obligations_empty_name_keeps_task_and_criterion_references() {
    let mut def = definition();
    task(&mut def, "T1").brief = " \n ".into();
    assert_eq!(
        task_obligations("T1", &def.tasks["T1"]),
        "C1, C9: only Contact form, task T1's assigned contribution per its brief"
    );
    task(&mut def, "T1").name.clear();
    assert_eq!(
        task_obligations("T1", &def.tasks["T1"]),
        "C1, C9: only T1, task T1's assigned contribution per its brief"
    );
}

#[test]
fn worker_goal_blank_task_override_uses_workers_template() {
    let mut def = definition();
    task(&mut def, "T1").goal = " \n ".into();
    assert!(goal(&def, "web-T1").contains("task T1 (Contact form) is accepted:"));
}

#[test]
fn worker_goal_blank_session_uses_seat_trailer_prefix() {
    let mut def = definition();
    task(&mut def, "T1").session = Some(" \n ".into());
    assert!(goal(&def, "web-T1").contains("`Fleet-Seat: web-T1 (`"));
}

#[test]
fn seat_goal_template_reference_and_unknown_seats_have_none() {
    let def = definition();
    for seat in ["template-web", "reference", "web-T7"] {
        assert_eq!(seat_goal(&def, seat), None, "{seat}");
    }
}

#[test]
fn brief_line_worker_carries_exact_bounded_goal() {
    let def = definition();
    let line = brief_line(&def, "web-T2");
    assert!(line.starts_with("\n\nYour goal, which the fleet-comms mod arms"));
    assert!(line.ends_with(&goal(&def, "web-T2")));
    assert_eq!(brief_line(&def, "observer"), "");
    assert_eq!(brief_line(&def, "template-web"), "");
}

#[test]
fn seat_goal_no_configured_goals_remains_disabled() {
    let mut def = definition();
    def.goals = GoalsDefinition::default();
    task(&mut def, "T2").goal.clear();
    for seat in ["coordinator", "acceptance", "observer", "web-T1", "web-T2"] {
        assert_eq!(seat_goal(&def, seat), None, "{seat}");
    }
}

#[test]
fn seat_goal_blank_configured_goals_remains_disabled() {
    let mut def = definition();
    def.goals.workers = " \n ".into();
    task(&mut def, "T2").goal = " \t ".into();
    assert_eq!(seat_goal(&def, "web-T1"), None);
    assert_eq!(seat_goal(&def, "web-T2"), None);
}

#[test]
fn worker_goal_large_brief_and_criterion_remain_referenced_without_inlining() {
    let mut def = definition();
    task(&mut def, "T1").brief = "exercise ".repeat(2_000);
    def.criteria
        .get_mut("C1")
        .expect("fixture criterion exists")
        .text = "global ".repeat(2_000);
    let text = goal(&def, "web-T1");
    assert!(text.chars().count() < 1_000);
    assert!(text.contains("task T1's assigned contribution per its brief"));
    assert!(!text.contains("exercise exercise"));
    assert!(!text.contains("global global"));
}

#[test]
fn task_obligations_no_criteria_names_assigned_contribution() {
    let def = definition();
    assert_eq!(
        task_obligations("T2", &def.tasks["T2"]),
        "Pricing: task T2's assigned contribution per its brief"
    );
}

#[test]
fn goal_validation_braces_in_task_name_are_not_template_placeholders() {
    use crate::launch::fleet::{checks::check_definition, tests::changed};
    let def = changed(serde_json::json!({
        "goals": {"workers": "report {task}: {criteria}"},
        "tasks": {"P1": {"name": "Render {field} syntax"}}
    }));
    assert_eq!(check_definition(&def), vec![]);
}

#[test]
fn goal_validation_unknown_template_placeholder_is_rejected_before_expansion() {
    use crate::launch::fleet::{checks::check_definition, tests::changed};
    let def = changed(serde_json::json!({
        "goals": {"workers": "report {typo}: {criteria}"}
    }));
    let found = check_definition(&def);
    assert!(
        found
            .iter()
            .any(|missing| missing.problem.contains("uses {typo}"))
    );
}
