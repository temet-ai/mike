use std::collections::HashMap;

use serde_json::Value;

use super::cli::{DEFAULT_TIMEOUT, HcCli, argv, str_field};
use super::terminal;
use crate::launch::backend::RunSeat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub space: String,
    pub tab: String,
    pub placement: String,
    pub reused: bool,
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::new_v4().simple())
}

pub fn run_space_id(run_id: &str) -> String {
    let safe: String = run_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("spmike-{safe}")
}

pub async fn place(
    cli: &HcCli,
    caller: Option<&str>,
    folder: &str,
    run: Option<&RunSeat>,
) -> Place {
    let tab = new_id("tb");
    if let Some(run) = run {
        let (space, reused) = run_space(run);
        return Place {
            space,
            tab,
            placement: "tab (the run's space)".to_string(),
            reused,
        };
    }
    let candidates = spaces_for_folder(cli, folder).await;
    let caller_space = match caller {
        Some(address) => terminal::get(cli, address)
            .await
            .ok()
            .flatten()
            .and_then(|t| t.space),
        None => None,
    };
    choose_space(caller_space, &candidates, tab)
}

fn run_space(run: &RunSeat) -> (String, bool) {
    match run.workspace_ref.as_deref().filter(|w| !w.is_empty()) {
        Some(space) => (space.to_string(), true),
        None => (run_space_id(&run.run_id), false),
    }
}

pub fn choose_space(caller_space: Option<String>, candidates: &[String], tab: String) -> Place {
    if let Some(space) = caller_space.filter(|s| candidates.contains(s)) {
        return Place {
            space,
            tab,
            placement: "tab (the caller's space, which is in this folder)".to_string(),
            reused: true,
        };
    }
    if let Some(space) = candidates.first() {
        return Place {
            space: space.clone(),
            tab,
            placement: "tab (the space already in this folder)".to_string(),
            reused: true,
        };
    }
    Place {
        space: new_id("sp"),
        tab,
        placement: "space (no space was in this folder)".to_string(),
        reused: false,
    }
}

async fn spaces_for_folder(cli: &HcCli, folder: &str) -> Vec<String> {
    let Ok(spaces) = cli.app_json(&argv(&["ls"]), DEFAULT_TIMEOUT).await else {
        return Vec::new();
    };
    let Ok(terminals) = cli.hc_json(&argv(&["ls", "--json"]), DEFAULT_TIMEOUT).await else {
        return Vec::new();
    };
    spaces_matching(&spaces, &terminals, folder)
}

pub fn spaces_matching(app_ls: &Value, hc_ls: &Value, folder: &str) -> Vec<String> {
    let cwds = terminal_cwds(hc_ls);
    let mut matched = Vec::new();
    for space in app_ls
        .get("spaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if space.get("machine").is_some_and(|m| !m.is_null()) {
            continue;
        }
        let Some(id) = str_field(space, "id") else {
            continue;
        };
        let in_folder = space_terminals(space)
            .filter_map(|t| cwds.get(t))
            .any(|cwd| within(cwd, folder));
        if in_folder {
            matched.push(id.to_string());
        }
    }
    matched
}

fn space_terminals(space: &Value) -> impl Iterator<Item = &str> {
    space
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|tab| {
            tab.get("terminals")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|t| str_field(t, "terminal"))
}

fn terminal_cwds(hc_ls: &Value) -> HashMap<&str, &str> {
    hc_ls
        .get("terminals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| Some((str_field(t, "address")?, str_field(t, "cwd")?)))
        .collect()
}

fn within(cwd: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches('/');
    cwd == folder
        || cwd
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}
