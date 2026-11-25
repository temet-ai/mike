use chrono::Duration;
use serde_json::json;

use super::session_judge::{TurnForm, render_session_judge};
use crate::dossier::view::full_tests::{base_time, fixture};
use crate::dossier::view::types::{DossierInputs, ReactionItem};
use crate::store::persistence::dossier_entries::DossierEntryRow;
use crate::store::persistence::dossier_view::{
    RecapRow, RecordTurnRow, SessionRecord, SuppliedLessonRow,
};

fn turn(
    id: i64,
    minute: i64,
    kind: Option<&str>,
    prompt: &str,
    stop: Option<&str>,
) -> RecordTurnRow {
    RecordTurnRow {
        turn_id: id,
        opened_at: base_time() + Duration::minutes(minute),
        opened_by: Some(
            if kind == Some("genuine") {
                "operator"
            } else {
                "machine"
            }
            .to_owned(),
        ),
        prompt_kind: kind.map(str::to_owned),
        prompt: prompt.to_owned(),
        stop_text: stop.map(str::to_owned),
        last_text: None,
        stop_reason: Some("end_turn".to_owned()),
        aborted: Some(false),
        interrupted: Some(false),
        ended_on: None,
        checks_count: Some(0),
        checks_failed: Some(0),
        files_touched: 0,
        tool_names: Vec::new(),
        tool_counts: Vec::new(),
        tool_errors: 0,
        commit_count: 0,
        commit_commands: Vec::new(),
        push_count: 0,
        push_commands: Vec::new(),
    }
}

fn operator(id: i64, minute: i64, prompt: &str, stop: Option<&str>) -> RecordTurnRow {
    turn(id, minute, Some("genuine"), prompt, stop)
}

fn inputs(turns: Vec<RecordTurnRow>) -> DossierInputs {
    let mut inputs = fixture();
    inputs.entries = Vec::new();
    inputs.record = SessionRecord {
        turns,
        recaps: Vec::new(),
        lessons: Vec::new(),
    };
    inputs
}

fn recap(request_id: i64, minute: i64, text: &str) -> RecapRow {
    RecapRow {
        request_id,
        at: base_time() + Duration::minutes(minute),
        text: text.to_owned(),
    }
}

fn lesson(lesson_id: i64, minute: i64, text: &str) -> SuppliedLessonRow {
    SuppliedLessonRow {
        lesson_id,
        text: text.to_owned(),
        route: "proxy".to_owned(),
        request_id: 500 + lesson_id,
        turn_id: Some(1),
        supplied_at: base_time() + Duration::minutes(minute),
    }
}

fn reaction(turn_id: i64, code: &str) -> ReactionItem {
    ReactionItem {
        verdict_id: turn_id,
        turn_id,
        reaction: code.to_owned(),
        delivery: None,
        legacy_label: None,
        confidence: Some(0.9),
        rationale: None,
        created_at: base_time(),
        bundle_id: None,
    }
}

fn entry(kind: &str, status: &str, turn_id: i64) -> DossierEntryRow {
    DossierEntryRow {
        id: turn_id,
        session_id: 1,
        operator_turn_id: Some(turn_id),
        kind: kind.to_owned(),
        data: json!({"class": "commit", "polarity": "allow", "what": "report back"}),
        status: status.to_owned(),
        source: "capture".to_owned(),
        source_ref: format!("requests.requests:{turn_id}"),
        dedup_key: format!("{kind}:{turn_id}"),
        extractor_version: "v1".to_owned(),
        effective_at: base_time(),
        recorded_at: base_time(),
        closed_at: None,
        close_recorded_at: None,
        closed_by: None,
    }
}

fn report<'a>(
    rendered: &'a super::session_judge::SessionJudgeRender,
    id: &str,
) -> anyhow::Result<&'a super::SectionReport> {
    rendered
        .reports
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| anyhow::anyhow!("no {id} report"))
}

fn block_of<'a>(text: &'a str, header: &str) -> &'a str {
    text.split(header)
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .unwrap_or_default()
}

fn adds_up(report: &super::SectionReport) -> bool {
    report.items_full + report.items_compressed + report.items_in_spans == report.items_total
}

#[test]
fn render_session_judge_small_session_matches_the_layout() {
    let mut inputs = inputs(vec![
        RecordTurnRow {
            files_touched: 2,
            tool_names: vec!["Edit".to_owned(), "Bash".to_owned()],
            tool_counts: vec![3, 2],
            commit_count: 1,
            commit_commands: vec!["git commit -m \"retry\"".to_owned()],
            checks_count: Some(1),
            ended_on: Some("report".to_owned()),
            ..operator(
                1,
                0,
                "Add a retry to the http client",
                Some("Done. Retry added with backoff."),
            )
        },
        turn(
            2,
            30,
            Some("task-notification"),
            "[SYSTEM NOTIFICATION] build finished",
            None,
        ),
    ]);
    inputs.reactions = vec![reaction(1, "accepted")];
    inputs.record.recaps = vec![recap(70, 20, "Retry added, build running.")];
    let rendered = render_session_judge(&inputs, 60_000);
    let expected = "SESSION
project: /repo
first genuine prompt: 2026-09-20T10:00:00+00:00
last genuine prompt: 2026-09-20T10:00:00+00:00
genuine turns: 1
machine-opened turns: 1
session class: operator (default; facts built 2026-09-20T10:00:00+00:00)

#1 10:00 operator
PROMPT:
Add a retry to the http client
STOPPED WITH:
Done. Retry added with backoff.
DID: files 2, commits 1, pushes 0, tools Edit x3 Bash x2, errors 0, checks failed 0 of 1, stop_reason=end_turn, ended_on=report, aborted=false, interrupted=false
  commit: git commit -m \"retry\"
REACTION: accepted

#2 10:30 background notification
PROMPT:
[SYSTEM NOTIFICATION] build finished
STOPPED WITH:
(no text: stop_reason=end_turn, aborted=false, interrupted=false)
DID: files 0, commits 0, pushes 0, tools none, errors 0, checks failed 0 of 0, stop_reason=end_turn, ended_on=none, aborted=false, interrupted=false

RECAPS (Claude's own account while the operator was away; it may be wrong):
- 2026-09-20 10:20: Retry added, build running.

LESSONS SUPPLIED (newest 20, each lesson once):
(none)

PROMISES (open or broken as of now):
(none)

DIRECTIVES IN FORCE:
(none)";
    assert_eq!(rendered.text, expected);
}

#[test]
fn render_session_judge_machine_turn_prompt_is_cut_to_300_and_labelled() {
    let long = format!(
        "<task-notification>{}</task-notification>",
        "w ".repeat(400)
    );
    let inputs = inputs(vec![
        operator(1, 0, &"o".repeat(500), Some("ok")),
        turn(2, 10, Some("loop-wakeup"), &long, Some("woke")),
        turn(
            3,
            20,
            Some("peer-message"),
            "hello from a peer",
            Some("replied"),
        ),
    ]);
    let text = render_session_judge(&inputs, 60_000).text;
    assert!(text.contains("#2 10:10 wakeup\nPROMPT:\n<task-notification>"));
    assert!(text.contains("#3 10:20 peer message"));
    assert!(text.contains(&format!("{}\nSTOPPED", "o".repeat(500))));
    let machine_prompt = text
        .split("#2 10:10 wakeup\nPROMPT:\n")
        .nth(1)
        .and_then(|rest| rest.split("\nSTOPPED WITH:").next())
        .unwrap_or_default();
    assert!(machine_prompt.chars().count() <= 300 + " [... 9999 more characters]".len());
    assert!(machine_prompt.contains("more characters]"));
}

#[test]
fn render_session_judge_empty_stop_request_shows_the_last_text_of_the_turn() {
    let mut empty_stop = operator(1, 0, "ask", None);
    empty_stop.last_text = Some("I will run the tests next.".to_owned());
    let text = render_session_judge(&inputs(vec![empty_stop]), 60_000).text;
    assert!(text.contains(
        "STOPPED WITH:\n(the last request wrote no text; last text in the turn:)\nI will run the tests next.\nDID:"
    ));
}

#[test]
fn render_session_judge_no_text_at_all_names_the_stop_facts() {
    let mut silent = operator(1, 0, "ask", None);
    silent.stop_reason = Some("max_tokens".to_owned());
    silent.aborted = Some(true);
    let text = render_session_judge(&inputs(vec![silent]), 60_000).text;
    assert!(text.contains(
        "STOPPED WITH:\n(no text: stop_reason=max_tokens, aborted=true, interrupted=false)"
    ));
}

#[test]
fn render_session_judge_stop_text_longer_than_3000_is_tail_cut() {
    let stop = format!("{}{}", "a".repeat(2_000), "b".repeat(3_000));
    let text = render_session_judge(&inputs(vec![operator(1, 0, "ask", Some(&stop))]), 60_000).text;
    assert!(text.contains("[... 2000 earlier characters not shown] "));
    assert!(text.contains(&"b".repeat(3_000)));
    assert!(!text.contains("aaaa"));
}

#[test]
fn render_session_judge_over_budget_shortens_oldest_turns_and_keeps_stop_text() -> anyhow::Result<()>
{
    let turns: Vec<RecordTurnRow> = (1..=30)
        .map(|i| {
            let stop = format!("{} CLOSING-{i}", "x".repeat(2_500));
            operator(
                i,
                i * 5,
                &format!("step {i} {}", "p".repeat(400)),
                Some(&stop),
            )
        })
        .collect();
    let rendered = render_session_judge(&inputs(turns), 40_000);
    assert!(rendered.text.chars().count() <= 40_000);
    for i in [1, 15, 30] {
        assert!(rendered.text.contains(&format!("CLOSING-{i}")), "turn {i}");
    }
    let forms: Vec<TurnForm> = rendered.view.forms.iter().map(|(_, f)| *f).collect();
    assert_eq!(forms.last(), Some(&TurnForm::Full));
    assert_eq!(forms.first(), Some(&TurnForm::Compressed));
    let oldest = block_of(&rendered.text, "#1 10:05 operator");
    assert!(!oldest.contains(&"p".repeat(300)));
    assert!(oldest.contains("DID: files 0, commits 0, pushes 0, errors 0, checks failed 0 of 0"));
    assert!(!oldest.contains("stop_reason="));
    let ending = block_of(&rendered.text, "#30 12:30 operator");
    assert!(ending.contains(&"p".repeat(400)));
    assert!(ending.contains(&"x".repeat(2_500)));
    assert!(adds_up(report(&rendered, "turns")?));
    Ok(())
}

#[test]
fn render_session_judge_shortened_turn_keeps_at_most_600_chars_of_stop_text() {
    let turns: Vec<RecordTurnRow> = (1..=20)
        .map(|i| operator(i, i, "ask", Some(&format!("{}END-{i}", "y".repeat(2_900)))))
        .collect();
    let text = render_session_judge(&inputs(turns), 30_000).text;
    let first = block_of(&text, "#1 10:01 operator");
    assert!(first.contains("END-1"));
    assert!(first.contains("[... 2305 earlier characters not shown]"));
    assert!(!first.contains(&"y".repeat(601)));
}

#[test]
fn render_session_judge_tiny_budget_merges_old_turns_but_never_drops_the_ending_stop_text()
-> anyhow::Result<()> {
    let turns: Vec<RecordTurnRow> = (1..=12)
        .map(|i| {
            operator(
                i,
                i,
                "ask",
                Some(&format!("{}FINAL-{i}", "z".repeat(1_000))),
            )
        })
        .collect();
    let rendered = render_session_judge(&inputs(turns), 2_500);
    assert!(rendered.text.contains("FINAL-12"));
    let report = report(&rendered, "turns")?;
    assert!(report.items_in_spans > 0);
    assert!(adds_up(report));
    assert!(rendered.text.contains("turns 1-"));
    assert_eq!(rendered.view.ending_turn_id, Some(12));
    Ok(())
}

#[test]
fn render_session_judge_recaps_are_dated_and_flagged_as_claudes_own_account() {
    let mut inputs = inputs(vec![operator(1, 0, "ask", Some("done"))]);
    inputs.record.recaps = vec![recap(70, 90, "Working on the retry.")];
    let text = render_session_judge(&inputs, 60_000).text;
    assert!(text.contains(
        "RECAPS (Claude's own account while the operator was away; it may be wrong):\n- 2026-09-20 11:30: Working on the retry."
    ));
}

#[test]
fn render_session_judge_empty_sections_say_none() -> anyhow::Result<()> {
    let rendered = render_session_judge(&inputs(vec![operator(1, 0, "ask", Some("done"))]), 60_000);
    for heading in [
        "RECAPS (Claude's own account while the operator was away; it may be wrong):",
        "LESSONS SUPPLIED (newest 20, each lesson once):",
        "PROMISES (open or broken as of now):",
        "DIRECTIVES IN FORCE:",
    ] {
        assert!(
            rendered.text.contains(&format!("{heading}\n(none)")),
            "{heading}"
        );
    }
    for id in ["recaps", "lessons", "promises", "directives"] {
        assert_eq!(report(&rendered, id)?.items_total, 0);
    }
    Ok(())
}

#[test]
fn render_session_judge_lessons_name_route_and_turn() {
    let mut inputs = inputs(vec![operator(1, 0, "ask", Some("done"))]);
    inputs.record.lessons = vec![lesson(7, 5, "Run clippy before pushing.")];
    let text = render_session_judge(&inputs, 60_000).text;
    assert!(text.contains("- lesson 7 via proxy on #1: Run clippy before pushing."));
}

#[test]
fn render_session_judge_promises_and_directives_list_only_what_is_in_force() -> anyhow::Result<()> {
    let mut inputs = inputs(vec![operator(1, 0, "ask", Some("done"))]);
    inputs.entries = vec![
        entry("promise", "open", 1),
        entry("promise", "kept", 1),
        entry("directive", "open", 1),
        entry("directive", "closed", 1),
    ];
    let rendered = render_session_judge(&inputs, 60_000);
    assert!(
        rendered
            .text
            .contains("PROMISES (open or broken as of now):\n- #1 open: ")
    );
    assert!(!rendered.text.contains("kept:"));
    assert!(
        rendered
            .text
            .contains("DIRECTIVES IN FORCE:\n- commit allow (requests.requests:1)")
    );
    assert_eq!(report(&rendered, "promises")?.items_total, 1);
    assert_eq!(report(&rendered, "directives")?.items_total, 1);
    Ok(())
}

#[test]
fn render_session_judge_over_budget_cuts_oldest_recaps_and_lessons_after_turns_are_shortened()
-> anyhow::Result<()> {
    let mut inputs = inputs(vec![operator(1, 0, "ask", Some("done"))]);
    inputs.record.recaps = (1..=12)
        .map(|i| recap(i, i, &format!("RECAP-{i} {}", "r".repeat(700))))
        .collect();
    inputs.record.lessons = (1..=12)
        .map(|i| lesson(i, i, &format!("LESSON-{i} {}", "l".repeat(350))))
        .collect();
    let rendered = render_session_judge(&inputs, 9_000);
    assert!(rendered.text.chars().count() <= 9_000);
    assert!(rendered.text.contains("RECAP-12"));
    assert!(!rendered.text.contains("RECAP-1 "));
    assert!(rendered.text.contains("earlier recaps are not shown)"));
    let recaps = report(&rendered, "recaps")?;
    assert!(recaps.items_in_spans > 0);
    assert!(adds_up(recaps));
    assert!(adds_up(report(&rendered, "lessons")?));
    Ok(())
}

#[test]
fn turns_text_drops_recaps_lessons_promises_and_directives() {
    let mut inputs = inputs(vec![operator(1, 0, "ask", Some("done"))]);
    inputs.record.recaps = vec![recap(70, 20, "Retry added.")];
    inputs.record.lessons = vec![lesson(7, 5, "Run clippy.")];
    let text = render_session_judge(&inputs, 60_000).text;
    let turns = super::session_judge::turns_text(&text);
    assert!(turns.ends_with("interrupted=false"));
    assert!(turns.contains("STOPPED WITH:\ndone"));
    assert!(!turns.contains("RECAPS"));
    assert!(!turns.contains("Run clippy."));
    assert_eq!(
        super::session_judge::turns_text("no sections"),
        "no sections"
    );
}

#[test]
fn render_session_judge_turn_without_a_reaction_has_no_reaction_line() {
    let text =
        render_session_judge(&inputs(vec![operator(1, 0, "ask", Some("done"))]), 60_000).text;
    assert!(!text.contains("REACTION:"));
}

#[test]
fn render_session_judge_commit_and_push_commands_are_cut_to_160_chars() {
    let mut turn = operator(1, 0, "ask", Some("done"));
    turn.commit_count = 4;
    turn.commit_commands = vec![format!("git commit -m \"{}\"", "m".repeat(300))];
    turn.push_count = 1;
    turn.push_commands = vec!["git push origin main".to_owned()];
    let text = render_session_judge(&inputs(vec![turn]), 60_000).text;
    assert!(text.contains("commits 4, pushes 1"));
    assert!(text.contains("\n  push: git push origin main"));
    let line = text
        .lines()
        .find(|l| l.starts_with("  commit: "))
        .unwrap_or_default();
    assert!(line.contains("[... "));
    assert!(line.chars().count() < "  commit: ".len() + 160 + 30);
}
