use serde_json::Value;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};
use super::surface::PaneRef;
use crate::launch::backend::RunSeat;
use crate::launch::error::LaunchError;

pub(super) const MIN_COLUMNS: u64 = 100;
pub(super) const MIN_ROWS: u64 = 30;
const WIDE_RATIO: u64 = 3;
const CONTROL_SEATS: [&str; 4] = ["coordinator", "observer", "acceptance", "status"];
const TEMPLATE_SEAT: &str = "template";
const METADATA_SOURCE: &str = "mike";

static PLACING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) struct Placed {
    pub(super) pane: PaneRef,
    pub(super) placement: String,
    pub(super) created_workspace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PaneRect {
    pub(super) pane: String,
    pub(super) width: u64,
    pub(super) height: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cut {
    Right,
    Down,
}

impl Cut {
    fn as_str(self) -> &'static str {
        match self {
            Cut::Right => "right",
            Cut::Down => "down",
        }
    }
}

pub(super) fn tab_label(run: &RunSeat) -> &'static str {
    let seat = run.seat.as_deref().unwrap_or_default();
    let role = run.role.as_deref().unwrap_or_default();
    if seat.starts_with(TEMPLATE_SEAT) {
        return "templates";
    }
    let control = CONTROL_SEATS
        .iter()
        .any(|name| seat.contains(name) || role.contains(name));
    if control { "control" } else { "workers" }
}

pub(super) fn choose_cut(panes: &[PaneRect]) -> Option<(String, Cut)> {
    let mut by_size: Vec<&PaneRect> = panes.iter().collect();
    by_size.sort_by_key(|p| std::cmp::Reverse(p.width * p.height));
    by_size
        .into_iter()
        .find_map(|p| cut_for(p).map(|cut| (p.pane.clone(), cut)))
}

fn cut_for(pane: &PaneRect) -> Option<Cut> {
    let right = pane.width / 2 >= MIN_COLUMNS;
    let down = pane.height / 2 >= MIN_ROWS;
    let prefers_right = pane.width >= pane.height * WIDE_RATIO;
    match (right, down) {
        (true, true) if prefers_right => Some(Cut::Right),
        (true, true) => Some(Cut::Down),
        (true, false) => Some(Cut::Right),
        (false, true) => Some(Cut::Down),
        (false, false) => None,
    }
}

pub(super) async fn place(
    cli: &HerdrCli,
    run: &RunSeat,
    folder: &str,
    env: &[String],
) -> Result<Placed, LaunchError> {
    let label = tab_label(run);
    let _turn = PLACING.lock().await;
    let placed = match find_workspace(cli, run).await? {
        Some(workspace) => place_in_workspace(cli, &workspace, label, folder, env).await?,
        None => create_workspace(cli, run, label, folder, env).await?,
    };
    name_pane(cli, &placed.pane.pane, run).await;
    Ok(placed)
}

async fn find_workspace(cli: &HerdrCli, run: &RunSeat) -> Result<Option<String>, LaunchError> {
    let result = cli
        .run_json(&argv(&["workspace", "list"]), DEFAULT_TIMEOUT)
        .await?;
    let workspaces = array(&result, "workspaces", "herdr workspace list")?;
    let recorded = run.workspace_ref.as_deref();
    let found = workspaces
        .iter()
        .find(|w| recorded.is_some() && str_field(w, "workspace_id") == recorded)
        .or_else(|| {
            workspaces
                .iter()
                .find(|w| str_field(w, "label") == Some(run.run_id.as_str()))
        });
    Ok(found
        .and_then(|w| str_field(w, "workspace_id"))
        .map(str::to_string))
}

async fn create_workspace(
    cli: &HerdrCli,
    run: &RunSeat,
    label: &str,
    folder: &str,
    env: &[String],
) -> Result<Placed, LaunchError> {
    let mut args = argv(&[
        "workspace",
        "create",
        "--cwd",
        folder,
        "--label",
        &run.run_id,
    ]);
    args.extend_from_slice(env);
    args.push("--no-focus".to_string());
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    let pane = root_pane(&result, "herdr workspace create")?;
    quietly(cli, argv(&["tab", "rename", &pane.tab, label])).await;
    Ok(Placed {
        pane,
        placement: format!("run workspace, new; tab {label}"),
        created_workspace: true,
    })
}

async fn place_in_workspace(
    cli: &HerdrCli,
    workspace: &str,
    label: &str,
    folder: &str,
    env: &[String],
) -> Result<Placed, LaunchError> {
    let tabs = tabs_labelled(cli, workspace, label).await?;
    let panes = panes_in(cli, workspace).await?;
    for tab in &tabs {
        let Some(first) = panes.iter().find(|p| &p.tab == tab) else {
            continue;
        };
        let rects = layout(cli, &first.pane).await?;
        if let Some((pane, cut)) = choose_cut(&rects) {
            return split(cli, &pane, cut, label, folder, env).await;
        }
    }
    let name = match tabs.len() {
        0 => label.to_string(),
        n => format!("{label} {}", n + 1),
    };
    create_tab(cli, workspace, &name, folder, env).await
}

async fn tabs_labelled(
    cli: &HerdrCli,
    workspace: &str,
    label: &str,
) -> Result<Vec<String>, LaunchError> {
    let args = argv(&["tab", "list", "--workspace", workspace]);
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    let numbered = format!("{label} ");
    Ok(array(&result, "tabs", "herdr tab list")?
        .iter()
        .filter(|t| str_field(t, "label").is_some_and(|l| l == label || l.starts_with(&numbered)))
        .filter_map(|t| str_field(t, "tab_id").map(str::to_string))
        .collect())
}

async fn panes_in(cli: &HerdrCli, workspace: &str) -> Result<Vec<PaneRef>, LaunchError> {
    let args = argv(&["pane", "list", "--workspace", workspace]);
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    Ok(array(&result, "panes", "herdr pane list")?
        .iter()
        .filter_map(PaneRef::from_value)
        .collect())
}

async fn layout(cli: &HerdrCli, pane: &str) -> Result<Vec<PaneRect>, LaunchError> {
    let args = argv(&["pane", "layout", "--pane", pane]);
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    let layout = result.get("layout").unwrap_or(&Value::Null);
    Ok(array(layout, "panes", "herdr pane layout")?
        .iter()
        .filter_map(pane_rect)
        .collect())
}

fn pane_rect(value: &Value) -> Option<PaneRect> {
    let rect = value.get("rect")?;
    Some(PaneRect {
        pane: str_field(value, "pane_id")?.to_string(),
        width: rect.get("width")?.as_u64()?,
        height: rect.get("height")?.as_u64()?,
    })
}

async fn split(
    cli: &HerdrCli,
    pane: &str,
    cut: Cut,
    label: &str,
    folder: &str,
    env: &[String],
) -> Result<Placed, LaunchError> {
    let mut args = argv(&[
        "pane",
        "split",
        "--pane",
        pane,
        "--direction",
        cut.as_str(),
        "--ratio",
        "0.5",
        "--cwd",
        folder,
    ]);
    args.extend_from_slice(env);
    args.push("--no-focus".to_string());
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    let pane = result
        .get("pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed("herdr pane split", &result))?;
    Ok(Placed {
        pane,
        placement: format!("run workspace; tab {label}, split {}", cut.as_str()),
        created_workspace: false,
    })
}

async fn create_tab(
    cli: &HerdrCli,
    workspace: &str,
    name: &str,
    folder: &str,
    env: &[String],
) -> Result<Placed, LaunchError> {
    let mut args = argv(&[
        "tab",
        "create",
        "--workspace",
        workspace,
        "--label",
        name,
        "--cwd",
        folder,
    ]);
    args.extend_from_slice(env);
    args.push("--no-focus".to_string());
    let result = cli.run_json(&args, DEFAULT_TIMEOUT).await?;
    Ok(Placed {
        pane: root_pane(&result, "herdr tab create")?,
        placement: format!("run workspace; new tab {name}"),
        created_workspace: false,
    })
}

async fn name_pane(cli: &HerdrCli, pane: &str, run: &RunSeat) {
    let Some(seat) = run.seat.as_deref().filter(|s| !s.is_empty()) else {
        return;
    };
    quietly(cli, argv(&["pane", "rename", pane, seat])).await;
    let token = format!("seat={seat}");
    let report = argv(&[
        "pane",
        "report-metadata",
        pane,
        "--source",
        METADATA_SOURCE,
        "--token",
        &token,
    ]);
    quietly(cli, report).await;
}

async fn quietly(cli: &HerdrCli, args: Vec<String>) {
    if let Err(err) = cli.run_json(&args, DEFAULT_TIMEOUT).await {
        tracing::debug!(command = %args.join(" "), %err, "herdr refused a display-only change");
    }
}

fn root_pane(result: &Value, command: &str) -> Result<PaneRef, LaunchError> {
    result
        .get("root_pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed(command, result))
}

fn array<'a>(value: &'a Value, key: &str, command: &str) -> Result<&'a Vec<Value>, LaunchError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| LaunchError::Malformed {
            command: command.to_string(),
            detail: format!("no `{key}` array in {value}"),
        })
}

fn malformed(command: &str, result: &Value) -> LaunchError {
    LaunchError::Malformed {
        command: command.to_string(),
        detail: format!("no pane ids in {result}"),
    }
}
