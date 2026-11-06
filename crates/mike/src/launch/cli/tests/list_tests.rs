use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::{assert_key_order, fake, runner};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::cli::list::{self, ListRow, merge};
use crate::launch::cli::render::print_list;
use crate::launch::{LaunchError, Surface};
use crate::store::persistence::launch::LaunchRow;

fn surface(pane: &str, session: Option<Uuid>, state: &str) -> Surface {
    let mut s = FakeBackend::surface(session);
    s.surface_ref = pane.to_string();
    s.agent_state = Some(state.to_string());
    s
}

pub(super) fn row(
    id: i64,
    session: Uuid,
    folder: &str,
    run: Option<&str>,
    seat: Option<&str>,
) -> LaunchRow {
    LaunchRow {
        id,
        session_uuid: Some(session),
        backend: "herdr".into(),
        backend_reason: None,
        folder: folder.into(),
        repo_root: None,
        worktree: false,
        workspace_ref: None,
        tab_ref: None,
        surface_ref: None,
        pid: None,
        run_id: run.map(str::to_string),
        seat: seat.map(str::to_string),
        template_id: None,
        model: None,
        agent: None,
        claude_args: serde_json::json!([]),
        prompt_present: false,
        brief_mode: None,
        brief_path: None,
        brief_text: None,
        brief_note_uuid: None,
        brief_read_at: None,
        requested_placement: None,
        actual_placement: None,
        split_direction: None,
        anchor_ref: None,
        remote_control: true,
        proxy_env: true,
        channel_check: None,
        channel_checked_at: None,
        opened: true,
        delivered: None,
        confirmed: false,
        result: "success".into(),
        reason: None,
        stderr_text: None,
        stderr_truncated: false,
        created_at: Utc
            .with_ymd_and_hms(2026, 9, 16, 8, 0, id as u32)
            .single()
            .unwrap(),
        opened_at: None,
        closed_at: None,
        close_reason: None,
        close_process_exited: None,
    }
}

#[test]
fn merge_ledger_rows_first_in_their_order_then_unknown_surfaces_by_id() {
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let rows = vec![
        row(1, a, "/proj/a", Some("run1"), Some("obs")),
        row(2, b, "/proj/b", Some("run1"), Some("dev")),
        row(3, Uuid::new_v4(), "/proj/gone", None, None),
    ];
    let surfaces = vec![
        surface("w1:p9", Some(c), "working"),
        surface("w1:p3", Some(b), "idle"),
        surface("w1:p1", None, "unknown"),
        surface("w1:p5", Some(a), "blocked"),
    ];
    let merged = merge(rows, surfaces, false);
    let order: Vec<&str> = merged.iter().map(|r| r.surface.as_str()).collect();
    assert_eq!(order, ["w1:p5", "w1:p3", "w1:p1", "w1:p9"]);
    assert_eq!(
        merged[0],
        ListRow {
            surface: "w1:p5".into(),
            folder: "/proj/a".into(),
            session: Some(a),
            agent_state: Some("blocked".into()),
            run: Some("run1".into()),
            seat: Some("obs".into()),
        }
    );
    assert_eq!(merged[2].folder, "/tmp/project");
    assert_eq!(merged[2].run, None);
    assert_eq!(
        merged[0].to_line(),
        format!("w1:p5\t/proj/a\t{a}\tblocked\trun1/obs")
    );
    assert_eq!(merged[2].to_line(), "w1:p1\t/tmp/project\t-\tunknown\t-/-");
}

#[test]
fn merge_run_scoped_lists_only_ledger_rows() {
    let a = Uuid::new_v4();
    let rows = vec![row(1, a, "/proj/a", Some("run1"), Some("obs"))];
    let surfaces = vec![
        surface("w1:p1", Some(Uuid::new_v4()), "idle"),
        surface("w1:p2", Some(a), "idle"),
    ];
    let merged = merge(rows, surfaces, true);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].surface, "w1:p2");
}

#[tokio::test]
async fn list_without_a_ledger_orders_surfaces_by_id() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    backend.list_answers.lock().await.push_back(Ok(vec![
        surface("w1:p3", Some(Uuid::new_v4()), "idle"),
        surface("w1:p1", None, "idle"),
        surface("w1:p2", Some(Uuid::new_v4()), "working"),
    ]));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let rows = list::execute(&runner, None).await.unwrap();
    let order: Vec<&str> = rows.iter().map(|r| r.surface.as_str()).collect();
    assert_eq!(order, ["w1:p1", "w1:p2", "w1:p3"]);
    assert_eq!(print_list(&rows, true), 0);
    assert_eq!(print_list(&rows, false), 0);
    let line = serde_json::to_string(&rows[0]).unwrap();
    assert_key_order(
        &line,
        &["surface", "folder", "session", "agent_state", "run", "seat"],
    );
}

#[tokio::test]
async fn list_backend_error_propagates() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    backend
        .list_answers
        .lock()
        .await
        .push_back(Err(LaunchError::Failed("herdr down".into())));
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    assert!(list::execute(&runner, None).await.is_err());
}
