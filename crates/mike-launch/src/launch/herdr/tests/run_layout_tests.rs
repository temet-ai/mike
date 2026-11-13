#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{FakeHerdr, agent_entry, envelope, pane_info, process_info};
use crate::launch::backend::{OpenRequest, Placement, RunSeat, TerminalBackend};
use crate::launch::herdr::HerdrBackend;
use crate::launch::herdr::run_layout::{Cut, PaneRect, choose_cut, tab_label};

const FOLDER: &str = "/tmp/project";
const RUN: &str = "fleet-demo-1a2b3c4d";

fn seat(seat: &str, role: &str, workspace: Option<&str>) -> RunSeat {
    RunSeat {
        run_id: RUN.into(),
        workspace_ref: workspace.map(str::to_string),
        seat: Some(seat.into()),
        role: Some(role.into()),
    }
}

fn request(run: RunSeat) -> OpenRequest {
    OpenRequest {
        folder: FOLDER.into(),
        placement: Placement::Tab,
        env: BTreeMap::from([("NO_PROXY".to_string(), "localhost".to_string())]),
        title: "cc·sonnet".into(),
        run_tab: None,
        run: Some(run),
        program: "claude".into(),
        args: vec!["--model".into(), "sonnet".into()],
        name: "cc-11111111".into(),
        prompt: None,
        prompt_wait_ms: 30_000,
    }
}

fn rect(pane: &str, width: u64, height: u64) -> PaneRect {
    PaneRect {
        pane: pane.into(),
        width,
        height,
    }
}

fn workspaces(entries: &[(&str, &str)]) -> String {
    let list: Vec<Value> = entries
        .iter()
        .map(|(id, label)| json!({"workspace_id": id, "label": label}))
        .collect();
    envelope(json!({"workspaces": list, "type": "workspace_list"}))
}

fn tabs(entries: &[(&str, &str)]) -> String {
    let list: Vec<Value> = entries
        .iter()
        .map(|(id, label)| json!({"tab_id": id, "label": label, "workspace_id": "w7"}))
        .collect();
    envelope(json!({"tabs": list, "type": "tab_list"}))
}

fn layout(panes: &[(&str, u64, u64)]) -> String {
    let list: Vec<Value> = panes
        .iter()
        .map(|(id, w, h)| json!({"pane_id": id, "rect": {"width": w, "height": h, "x": 0, "y": 0}}))
        .collect();
    envelope(json!({"layout": {"panes": list}, "type": "pane_layout"}))
}

fn started(fake: &FakeHerdr, pane: &str) {
    fake.answer("pane rename", &envelope(json!({"type": "ok"})))
        .answer("pane report-metadata", &envelope(json!({"type": "ok"})))
        .answer(
            "agent start",
            &envelope(json!({"agent": agent_entry(pane, "claude", None, "idle"), "type": "agent_started"})),
        )
        .answer(
            "pane process-info",
            &envelope(process_info(pane, 100, 200, Some(200))),
        );
}

#[test]
fn control_seats_go_to_the_control_tab_and_the_rest_to_workers() {
    assert_eq!(
        tab_label(&seat("coordinator", "coordinator", None)),
        "control"
    );
    assert_eq!(tab_label(&seat("observer-9", "observer", None)), "control");
    assert_eq!(tab_label(&seat("jc-coordinator-7", "", None)), "control");
    assert_eq!(
        tab_label(&seat("acceptance", "acceptance", None)),
        "control"
    );
    assert_eq!(
        tab_label(&seat("rust-developer-P2", "rust-developer", None)),
        "workers"
    );
    assert_eq!(
        tab_label(&seat("template-rust-developer", "rust-developer", None)),
        "templates"
    );
}

#[test]
fn a_wide_pane_is_cut_to_the_right_and_a_tall_one_down() {
    assert_eq!(
        choose_cut(&[rect("p1", 528, 89)]),
        Some(("p1".into(), Cut::Right))
    );
    assert_eq!(
        choose_cut(&[rect("p1", 132, 89)]),
        Some(("p1".into(), Cut::Down))
    );
    assert_eq!(
        choose_cut(&[rect("p1", 264, 89)]),
        Some(("p1".into(), Cut::Down))
    );
}

#[test]
fn four_control_seats_on_a_wide_terminal_make_a_two_by_two_grid() {
    let mut panes = vec![rect("p1", 528, 89)];
    let mut cuts = Vec::new();
    for n in 2..=4 {
        let (pane, cut) = choose_cut(&panes).unwrap();
        cuts.push(cut);
        let i = panes.iter().position(|p| p.pane == pane).unwrap();
        let (w, h) = (panes[i].width, panes[i].height);
        let (a, b) = match cut {
            Cut::Right => ((w / 2, h), (w - w / 2, h)),
            Cut::Down => ((w, h / 2), (w, h - h / 2)),
        };
        panes[i] = rect(&pane, a.0, a.1);
        panes.push(rect(&format!("p{n}"), b.0, b.1));
    }
    assert_eq!(cuts, vec![Cut::Right, Cut::Down, Cut::Down]);
    let mut sizes: Vec<(u64, u64)> = panes.iter().map(|p| (p.width, p.height)).collect();
    sizes.sort();
    assert_eq!(sizes, vec![(264, 44), (264, 44), (264, 45), (264, 45)]);
}

#[test]
fn the_largest_pane_is_cut_first_and_the_earlier_one_wins_a_tie() {
    assert_eq!(
        choose_cut(&[
            rect("p1", 132, 89),
            rect("p2", 264, 89),
            rect("p3", 132, 89)
        ]),
        Some(("p2".into(), Cut::Down))
    );
    assert_eq!(
        choose_cut(&[rect("p1", 264, 89), rect("p2", 264, 89)]),
        Some(("p1".into(), Cut::Down))
    );
}

#[test]
fn no_pane_is_cut_below_the_smallest_readable_size() {
    assert_eq!(
        choose_cut(&[rect("p1", 132, 44), rect("p2", 132, 45)]),
        None
    );
    assert_eq!(choose_cut(&[rect("p1", 199, 59)]), None);
}

#[tokio::test]
async fn the_first_seat_of_a_run_creates_the_workspace_and_names_its_tab() {
    let fake = FakeHerdr::new();
    fake.answer("workspace list", &workspaces(&[("w1", "temet-proxy")]))
        .answer(
            "workspace create",
            &envelope(json!({
                "workspace": {"workspace_id": "w7"},
                "tab": {"tab_id": "w7:t1"},
                "root_pane": pane_info("w7", "w7:t1", "w7:p1", FOLDER),
                "type": "workspace_created"
            })),
        )
        .answer("tab rename", &envelope(json!({"type": "ok"})));
    started(&fake, "w7:p1");
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(seat("coordinator", "coordinator", None)))
        .await
        .unwrap();
    assert_eq!(surface.workspace_ref.as_deref(), Some("w7"));
    assert_eq!(surface.surface_ref, "w7:p1");
    assert_eq!(surface.actual_placement, "run workspace, new; tab control");
    assert!(!surface.reused);
    let calls = fake.calls();
    assert_eq!(calls[0], vec!["workspace", "list"]);
    assert_eq!(
        calls[1],
        vec![
            "workspace",
            "create",
            "--cwd",
            FOLDER,
            "--label",
            RUN,
            "--env",
            "NO_PROXY=localhost",
            "--no-focus"
        ]
    );
    assert_eq!(calls[2], vec!["tab", "rename", "w7:t1", "control"]);
    assert_eq!(calls[3], vec!["pane", "rename", "w7:p1", "coordinator"]);
    assert_eq!(
        calls[4],
        vec![
            "pane",
            "report-metadata",
            "w7:p1",
            "--source",
            "mike",
            "--token",
            "seat=coordinator"
        ]
    );
}

#[tokio::test]
async fn a_later_worker_splits_the_largest_pane_of_the_workers_tab() {
    let fake = FakeHerdr::new();
    fake.answer(
        "workspace list",
        &workspaces(&[("w1", "temet-proxy"), ("w7", RUN)]),
    )
    .answer(
        "tab list",
        &tabs(&[("w7:t1", "control"), ("w7:t2", "workers")]),
    )
    .answer(
        "pane list",
        &envelope(json!({"panes": [
                pane_info("w7", "w7:t1", "w7:p1", FOLDER),
                pane_info("w7", "w7:t2", "w7:p2", FOLDER),
                pane_info("w7", "w7:t2", "w7:p3", FOLDER)
            ], "type": "pane_list"})),
    )
    .answer(
        "pane layout",
        &layout(&[("w7:p2", 264, 89), ("w7:p3", 264, 89)]),
    )
    .answer(
        "pane split",
        &envelope(json!({"pane": pane_info("w7", "w7:t2", "w7:p4", FOLDER), "type": "pane_split"})),
    );
    started(&fake, "w7:p4");
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(seat(
            "rust-developer-P2",
            "rust-developer",
            Some("w7"),
        )))
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w7:p4");
    assert_eq!(surface.tab_ref.as_deref(), Some("w7:t2"));
    assert_eq!(
        surface.actual_placement,
        "run workspace; tab workers, split down"
    );
    let calls = fake.calls();
    assert_eq!(calls[1], vec!["tab", "list", "--workspace", "w7"]);
    assert_eq!(calls[2], vec!["pane", "list", "--workspace", "w7"]);
    assert_eq!(calls[3], vec!["pane", "layout", "--pane", "w7:p2"]);
    assert_eq!(
        calls[4],
        vec![
            "pane",
            "split",
            "--pane",
            "w7:p2",
            "--direction",
            "down",
            "--ratio",
            "0.5",
            "--cwd",
            FOLDER,
            "--env",
            "NO_PROXY=localhost",
            "--no-focus"
        ]
    );
}

#[tokio::test]
async fn a_run_found_by_its_label_is_used_when_the_ledger_has_no_workspace_yet() {
    let fake = FakeHerdr::new();
    fake.answer("workspace list", &workspaces(&[("w7", RUN)]))
        .answer("tab list", &tabs(&[("w7:t1", "control")]))
        .answer(
            "pane list",
            &envelope(
                json!({"panes": [pane_info("w7", "w7:t1", "w7:p1", FOLDER)], "type": "pane_list"}),
            ),
        )
        .answer(
            "tab create",
            &envelope(json!({
                "root_pane": pane_info("w7", "w7:t2", "w7:p2", FOLDER),
                "tab": {"tab_id": "w7:t2"},
                "type": "tab_created"
            })),
        );
    started(&fake, "w7:p2");
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(seat("db-dev-P1", "db-dev", None)))
        .await
        .unwrap();
    assert_eq!(surface.workspace_ref.as_deref(), Some("w7"));
    assert_eq!(surface.actual_placement, "run workspace; new tab workers");
    let calls = fake.calls();
    assert_eq!(
        calls[3],
        vec![
            "tab",
            "create",
            "--workspace",
            "w7",
            "--label",
            "workers",
            "--cwd",
            FOLDER,
            "--env",
            "NO_PROXY=localhost",
            "--no-focus"
        ]
    );
}

#[tokio::test]
async fn a_full_workers_tab_starts_a_second_one() {
    let fake = FakeHerdr::new();
    fake.answer("workspace list", &workspaces(&[("w7", RUN)]))
        .answer(
            "tab list",
            &tabs(&[("w7:t1", "control"), ("w7:t2", "workers")]),
        )
        .answer(
            "pane list",
            &envelope(
                json!({"panes": [pane_info("w7", "w7:t2", "w7:p2", FOLDER)], "type": "pane_list"}),
            ),
        )
        .answer(
            "pane layout",
            &layout(&[("w7:p2", 132, 44), ("w7:p3", 132, 45)]),
        )
        .answer(
            "tab create",
            &envelope(json!({
                "root_pane": pane_info("w7", "w7:t3", "w7:p9", FOLDER),
                "tab": {"tab_id": "w7:t3"},
                "type": "tab_created"
            })),
        );
    started(&fake, "w7:p9");
    let surface = HerdrBackend::with_binary(fake.bin())
        .open(&request(seat(
            "rust-developer-P9",
            "rust-developer",
            Some("w7"),
        )))
        .await
        .unwrap();
    assert_eq!(surface.actual_placement, "run workspace; new tab workers 2");
    assert_eq!(surface.surface_ref, "w7:p9");
}

#[tokio::test]
async fn the_status_pane_goes_to_the_control_tab_and_runs_its_command() {
    use crate::launch::backend::PaneRequest;
    let fake = FakeHerdr::new();
    fake.answer("workspace list", &workspaces(&[("w1", "temet-proxy")]))
        .answer(
            "workspace create",
            &envelope(json!({
                "workspace": {"workspace_id": "w7"},
                "tab": {"tab_id": "w7:t1"},
                "root_pane": pane_info("w7", "w7:t1", "w7:p1", FOLDER),
                "type": "workspace_created"
            })),
        )
        .answer("tab rename", &envelope(json!({"type": "ok"})))
        .answer("pane rename", &envelope(json!({"type": "ok"})))
        .answer("pane report-metadata", &envelope(json!({"type": "ok"})))
        .answer("pane run", &envelope(json!({"type": "ok"})));
    let surface = HerdrBackend::with_binary(fake.bin())
        .open_pane(&PaneRequest {
            folder: FOLDER.into(),
            run: seat("status", "status", None),
            command: "mike fleet status demo --watch 5".into(),
        })
        .await
        .unwrap();
    assert_eq!(surface.surface_ref, "w7:p1");
    assert_eq!(surface.actual_placement, "run workspace, new; tab control");
    assert_eq!(surface.session_uuid, None);
    let calls = fake.calls();
    assert_eq!(calls[2], vec!["tab", "rename", "w7:t1", "control"]);
    assert_eq!(
        calls[5],
        vec!["pane", "run", "w7:p1", "mike fleet status demo --watch 5"]
    );
}
