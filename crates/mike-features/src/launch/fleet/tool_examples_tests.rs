use serde_json::{Value, json};
use uuid::Uuid;

use super::brief;

fn call(text: &str, name: &str) -> anyhow::Result<Value> {
    let prefix = format!("{name} ");
    let args = text
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .ok_or_else(|| anyhow::anyhow!("missing {name} example"))?;
    Ok(serde_json::from_str(args)?)
}

#[test]
fn worker_examples_name_the_callers_session_and_owned_task() -> anyhow::Result<()> {
    let session = Uuid::from_u128(9);
    let text = brief("fleet-id", session, Some("T3"));
    for name in ["ask", "answer"] {
        assert_eq!(call(&text, name)?["from"], session.to_string());
    }
    for name in ["write_session_note", "fleet_report"] {
        assert_eq!(call(&text, name)?["session_id"], session.to_string());
    }
    let report = call(&text, "fleet_report")?;
    assert_eq!(report["fleet"], "fleet-id");
    assert_eq!(report["task"], "T3");
    assert!(report.get("criteria").is_none());
    Ok(())
}

#[test]
fn a_seat_without_an_owned_task_has_no_report_example() -> anyhow::Result<()> {
    let text = brief("fleet-id", Uuid::from_u128(2), None);
    for name in ["ask", "answer", "write_session_note"] {
        assert!(call(&text, name)?.is_object());
    }
    assert!(!text.contains("fleet_report"));
    assert!(call(&text, "fleet_report").is_err());
    Ok(())
}

#[test]
fn example_arguments_preserve_quotes_backslashes_and_line_breaks() -> anyhow::Result<()> {
    let fleet = "fleet\"\\\nwith a line";
    let task_id = "task\"\\\nwith a line";
    let text = brief(fleet, Uuid::from_u128(3), Some(task_id));
    let report = call(&text, "fleet_report")?;
    assert_eq!(report["fleet"], fleet);
    assert_eq!(report["task"], task_id);
    assert!(report.get("criteria").is_none());
    assert_eq!(text.lines().count(), 6);
    Ok(())
}

#[test]
fn examples_leave_unknown_content_explicit_and_require_real_evidence() -> anyhow::Result<()> {
    let text = brief("fleet-id", Uuid::from_u128(4), Some("T3"));
    assert!(text.contains("replace <...> with real values"));
    assert!(text.contains("Do not call on startup"));
    assert!(text.contains("After fleet_commit"));
    assert!(text.contains("with real evidence"));
    let ask = call(&text, "ask")?;
    assert_eq!(ask["question"], "<question>");
    assert!(ask.get("blocked").is_none());
    let answer = call(&text, "answer")?;
    assert_eq!(answer["question_id"], "<question id>");
    assert!(answer.get("amend").is_none());
    let note = call(&text, "write_session_note")?;
    assert_eq!(note["title"], "<finding>");
    assert!(note.get("note_uuid").is_none());
    assert!(note.get("status").is_none());
    assert!(note.get("data").is_none());
    assert_eq!(
        call(&text, "fleet_report")?["evidence"],
        json!(["<evidence path>"])
    );
    Ok(())
}

#[test]
fn ordinary_examples_are_under_twelve_hundred_bytes_without_extra_instructions() {
    let text = brief("fleet-id", Uuid::from_u128(5), Some("T3"));
    assert!(text.len() < 1_200, "{} bytes", text.len());
    assert!(!text.contains("fleet_verdict"));
    assert!(!text.contains("ack_session_message"));
    assert!(!text.contains("register_workspace_session"));
}

#[test]
fn long_ids_are_whole_with_fixed_example_overhead() -> anyhow::Result<()> {
    let fleet = "fleet\"\\\n".repeat(1_000);
    let task = "task\"\\\n".repeat(1_000);
    let text = brief(&fleet, Uuid::from_u128(6), Some(&task));
    let report = call(&text, "fleet_report")?;
    assert_eq!(report["fleet"], fleet);
    assert_eq!(report["task"], task);
    let encoded_ids = json!(fleet).to_string().len() + json!(task).to_string().len();
    assert!(text.len() - encoded_ids < 1_200);
    Ok(())
}

#[test]
fn large_criteria_lists_do_not_grow_the_call_examples() -> anyhow::Result<()> {
    use crate::launch::fleet::{plan, plan_tests, tests};

    let mut def = tests::parsed(&tests::complete());
    def.tasks
        .get_mut("P2")
        .ok_or_else(|| anyhow::anyhow!("missing task"))?
        .criteria = (0..10_000).map(|id| format!("C{id}")).collect();
    let facts = plan_tests::facts();
    let session = Uuid::from_u128(7);
    let seat = plan::worker(&def, &facts, "P2", session)
        .ok_or_else(|| anyhow::anyhow!("missing worker"))?;
    let report = call(&seat.prompt, "fleet_report")?;
    assert_eq!(
        report,
        call(&brief(&facts.fleet, session, Some("P2")), "fleet_report")?
    );
    assert!(report.get("criteria").is_none());
    Ok(())
}

#[test]
fn inline_and_note_worker_briefs_fill_the_new_seats_own_identity() -> anyhow::Result<()> {
    use crate::launch::fleet::{plan, plan_tests, tests};

    let mut def = tests::parsed(&tests::complete());
    let facts = plan_tests::facts();
    for as_notes in [false, true] {
        def.briefs.as_notes = as_notes;
        for session in [Uuid::from_u128(50), Uuid::from_u128(51)] {
            let seat = plan::worker(&def, &facts, "P2", session)
                .ok_or_else(|| anyhow::anyhow!("missing worker"))?;
            for name in ["ask", "answer"] {
                assert_eq!(call(&seat.prompt, name)?["from"], session.to_string());
            }
            for name in ["write_session_note", "fleet_report"] {
                assert_eq!(call(&seat.prompt, name)?["session_id"], session.to_string());
            }
            let report = call(&seat.prompt, "fleet_report")?;
            assert_eq!(report["fleet"], facts.fleet);
            assert_eq!(report["task"], "P2");
            assert!(report.get("criteria").is_none());
            assert_eq!(seat.prompt.matches("Examples only;").count(), 1);
        }
    }
    Ok(())
}

#[test]
fn control_and_reference_briefs_use_their_own_ids_without_reporting() -> anyhow::Result<()> {
    use crate::launch::fleet::{plan, plan_tests, tests};

    let def = tests::parsed(&tests::complete());
    let facts = plan_tests::facts();
    let mut seats = Vec::new();
    let reference = plan::reference(&def, &facts, "db-dev", Uuid::from_u128(61))
        .ok_or_else(|| anyhow::anyhow!("missing reference"))?;
    assert!(
        reference
            .prompt
            .ends_with("End this first turn with the single line READY and wait for mail.")
    );
    seats.push(reference);
    for seat in seats {
        assert_eq!(call(&seat.prompt, "ask")?["from"], seat.session.to_string());
        assert_eq!(
            call(&seat.prompt, "answer")?["from"],
            seat.session.to_string()
        );
        assert_eq!(
            call(&seat.prompt, "write_session_note")?["session_id"],
            seat.session.to_string()
        );
        assert!(!seat.prompt.contains("fleet_report "));
        assert!(call(&seat.prompt, "fleet_report").is_err());
    }
    Ok(())
}

#[test]
fn templates_never_pass_their_own_caller_ids_to_forked_workers() -> anyhow::Result<()> {
    use crate::launch::fleet::{plan, plan_tests, tests};

    let mut def = tests::parsed(&tests::complete());
    let facts = plan_tests::facts();
    for as_notes in [false, true] {
        def.briefs.as_notes = as_notes;
        let template = plan::template(&def, &facts, "db-dev")
            .ok_or_else(|| anyhow::anyhow!("missing template"))?;
        assert!(!template.prompt.contains("Examples only;"));
        assert!(!template.prompt.contains(&template.session.to_string()));
        for name in ["ask", "answer", "write_session_note", "fleet_report"] {
            assert!(call(&template.prompt, name).is_err());
        }
    }
    Ok(())
}
