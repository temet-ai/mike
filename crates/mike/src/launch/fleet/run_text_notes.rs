use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;

use super::brief_notes::fleet_tag;
use super::bundle;
use super::definition::FleetDefinition;
use super::plan::FleetFacts;
use super::run_notebook::{self, RunNote};
use crate::store::persistence::PersistHandle;

pub(super) const RULES_TAG: &str = "rules";
const BUILT_IN: &str = "built into mike";

struct Text {
    tag: String,
    title: String,
    source: String,
    text: String,
}

fn source_of(path: &Path, state: &Path) -> String {
    if path.starts_with(state) {
        BUILT_IN.to_string()
    } else {
        format!("project file {}", path.display())
    }
}

fn rules_text(repo: &str, state: &Path, run: &str) -> Option<Text> {
    let read = bundle::resolve_file(repo, state, "rules.md").and_then(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        Some((source_of(&path, state), text))
    });
    let (source, text) = match read {
        Some(found) => found,
        None => (
            BUILT_IN.to_string(),
            bundle::generic_file("rules.md")?.to_string(),
        ),
    };
    Some(Text {
        tag: RULES_TAG.to_string(),
        title: format!("Fleet rules used by run {run}"),
        source,
        text,
    })
}

fn contract_text(repo: &str, state: &Path, file: &str, run: &str) -> Option<Text> {
    let path = bundle::resolve_contract(repo, state, file)?;
    let text = std::fs::read_to_string(&path).ok()?;
    let name = Path::new(file).file_name()?.to_str()?.to_string();
    Some(Text {
        title: format!("Seat contract {name} used by run {run}"),
        tag: format!("contract:{name}"),
        source: source_of(&path, state),
        text,
    })
}

fn texts(def: &FleetDefinition, state: &Path, run: &str) -> Vec<Text> {
    let mut found: BTreeMap<String, Text> = BTreeMap::new();
    let files = def.workers.values().map(|worker| worker.contract_file());
    for file in files {
        if let Some(text) = contract_text(&def.repo, state, &file, run) {
            found.entry(text.tag.clone()).or_insert(text);
        }
    }
    let mut all: Vec<Text> = rules_text(&def.repo, state, run).into_iter().collect();
    all.extend(found.into_values());
    all
}

fn note_of(text: Text, facts: &FleetFacts) -> RunNote {
    RunNote {
        title: text.title,
        body: format!("Source: {}\n\n{}", text.source, text.text),
        tags: vec![fleet_tag(&facts.fleet), facts.run_id.clone(), text.tag],
        data: json!({"run": facts.run_id, "source": text.source}),
    }
}

pub(super) async fn record_in(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    state: &Path,
) -> usize {
    let items = texts(def, state, &facts.run_id);
    if items.is_empty() {
        return 0;
    }
    let coordinator = Some(facts.seats.coordinator);
    let Some(session) = run_notebook::session(persist, coordinator, &def.repo, &facts.run_id).await
    else {
        return 0;
    };
    let mut written = 0;
    for text in items {
        if run_notebook::write_once(persist, session, note_of(text, facts)).await {
            written += 1;
        }
    }
    written
}

pub(super) async fn record(persist: &PersistHandle, (def, facts): (&FleetDefinition, &FleetFacts)) {
    let state = bundle::default_state_dir(&bundle::home());
    record_in(persist, (def, facts), &state).await;
}
