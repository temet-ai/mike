use chrono::{Duration, Utc};
use serde_json::json;

use super::{Attribution, author_of, change_view};
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::definition::TaskDefinition;
use crate::store::persistence::fleet::TreeChangeRow;

fn row() -> TreeChangeRow {
    TreeChangeRow {
        id: 1,
        path: "src/a.rs".into(),
        change: "modified".into(),
        content_sha: Some("blob-a".into()),
        seen_at: Utc::now(),
        seats: Some(vec!["lab-T1".into()]),
        named: None,
    }
}

fn edit(row: &TreeChangeRow, session: Option<i64>) -> ChangeEvent {
    ChangeEvent {
        id: 1,
        project_id: 1,
        checkout: "/repo".into(),
        at: row.seen_at,
        kind: ChangeKind::Edit,
        session_id: session,
        run_id: None,
        task: None,
        paths: vec![row.path.clone()],
        subject: String::new(),
        detail: json!({"author": "tool_call", "content_verified": true}),
        before_sha: None,
        after_sha: row.content_sha.clone(),
    }
}

fn definition() -> FleetDefinition {
    let mut def = FleetDefinition::default();
    def.tasks.insert(
        "T2".into(),
        TaskDefinition {
            files: vec!["src/a.rs".into()],
            seat: Some("lab-T2".into()),
            ..Default::default()
        },
    );
    def
}

#[test]
fn exact_content_verified_edit_names_its_author() {
    let row = row();
    assert_eq!(author_of(&row, &[edit(&row, Some(22))]).seat, Some(22));
}

#[test]
fn matching_content_beats_a_nearer_different_write() {
    let row = row();
    let mut matching = edit(&row, Some(22));
    matching.at -= Duration::seconds(8);
    let mut other = edit(&row, Some(11));
    other.after_sha = Some("different-blob".into());
    assert_eq!(author_of(&row, &[other, matching]).seat, Some(22));
}

#[test]
fn competing_matching_edits_preserve_candidates_without_selecting_nearest() {
    let row = row();
    let mut other = edit(&row, Some(11));
    other.at -= Duration::seconds(8);
    let actual = author_of(&row, &[other, edit(&row, Some(22))]);
    assert_eq!(actual.seat, None);
    assert_eq!(actual.candidates, vec![11, 22]);
}

#[test]
fn unknown_record_candidates_are_preserved_and_prevent_certainty() {
    let row = row();
    let mut unknown = edit(&row, None);
    unknown.detail = json!({"author": "unknown", "candidates": [11, 22]});
    let actual = author_of(&row, &[unknown, edit(&row, Some(22))]);
    assert_eq!(actual.seat, None);
    assert_eq!(actual.candidates, vec![11, 22]);
}

#[test]
fn historical_path_mentions_and_shell_overlap_are_not_proof() {
    let row = row();
    for detail in [json!({"author": "tool_call"}), json!({"author": "shell"})] {
        let mut event = edit(&row, Some(11));
        event.detail = detail;
        let actual = author_of(&row, &[event]);
        assert_eq!(actual.seat, None);
        assert_eq!(actual.candidates, vec![11]);
    }
}

#[test]
fn absent_content_cannot_prove_authorship() {
    let mut row = row();
    row.content_sha = None;
    assert_eq!(author_of(&row, &[edit(&row, Some(11))]).seat, None);
}

#[test]
fn another_path_or_time_cannot_prove_authorship() {
    let row = row();
    let mut other_path = edit(&row, Some(11));
    other_path.paths = vec!["src/b.rs".into()];
    let mut late = edit(&row, Some(22));
    late.at -= Duration::seconds(60);
    assert_eq!(author_of(&row, &[other_path, late]), Attribution::default());
}

#[test]
fn task_owner_is_separate_from_unknown_author() {
    let view = change_view(&definition(), row(), Attribution::default());
    assert_eq!(view.seat, None);
    assert_eq!(view.owner_seat.as_deref(), Some("lab-T2"));
    assert_eq!(view.task.as_deref(), Some("T2"));
    assert_eq!(view.attribution, "unknown");
    assert!(
        view.note
            .is_some_and(|n| n.contains("author unknown") && !n.contains("written by"))
    );
}

#[test]
fn read_only_path_mentions_and_sole_running_seats_are_candidates() {
    let mut row = row();
    row.named = Some(vec!["lab-T3".into()]);
    let view = change_view(&definition(), row, Attribution::default());
    assert_eq!(view.seat, None);
    assert_eq!(view.candidates, vec!["lab-T1", "lab-T3"]);
    assert!(
        view.note
            .is_some_and(|n| n.contains("do not prove a write"))
    );
}

#[test]
fn proven_author_alone_can_raise_an_outside_task_warning() {
    let author = Attribution {
        seat: Some("lab-T3".into()),
        candidates: vec!["lab-T3".into()],
    };
    let view = change_view(&definition(), row(), author);
    assert_eq!(view.seat.as_deref(), Some("lab-T3"));
    assert_eq!(view.attribution, "proven");
    assert!(
        view.note
            .is_some_and(|n| n == "written by lab-T3, but the path belongs to lab-T2")
    );
}

#[test]
fn unowned_and_unseated_paths_do_not_turn_running_commands_into_authors() -> anyhow::Result<()> {
    let mut def = definition();
    def.tasks
        .get_mut("T2")
        .ok_or_else(|| anyhow::anyhow!("task missing"))?
        .seat = None;
    for def in [def, FleetDefinition::default()] {
        let view = change_view(&def, row(), Attribution::default());
        assert_eq!(view.seat, None);
        assert_eq!(view.owner_seat, None);
        assert_eq!(view.attribution, "unknown");
    }
    Ok(())
}
