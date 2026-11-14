use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;

pub const POINT: &str = "blocked_dialog";
pub const QUESTION_SET: &str = "blocked_dialog_v1";
const WHICH: &str = "which";
const UNKNOWN: &str = "unknown";
const SCREEN_CHARS: usize = 1500;

const DIALOGS: &[(&str, &str, &str)] = &[
    (
        "development_channels",
        "the development-channels dialog",
        "The screen asks whether to load development channels.",
    ),
    (
        "folder_trust",
        "the folder trust dialog",
        "The screen asks whether to trust the files in this folder.",
    ),
    (
        "permission_prompt",
        "a tool permission prompt",
        "The screen asks whether to allow a tool, command or file edit, with numbered answers.",
    ),
    (
        "mcp_server_approval",
        "the MCP server approval dialog",
        "The screen asks whether to use a new MCP server from a project file.",
    ),
    (
        "login",
        "the login dialog",
        "The screen asks the person to sign in or choose a login method.",
    ),
    (
        "first_run_setup",
        "the first-run setup dialog",
        "The screen shows first-run setup such as choosing a theme or accepting terms.",
    ),
    (
        "bypass_permissions_warning",
        "the bypass-permissions warning",
        "The screen warns about running with permission checks bypassed and asks to accept.",
    ),
];

pub fn questions() -> BTreeMap<String, Question> {
    let mut options: Vec<(&str, &str)> = DIALOGS
        .iter()
        .map(|(k, _, meaning)| (*k, *meaning))
        .collect();
    options.push((
        UNKNOWN,
        "The screen shows a dialog that is none of these, or no dialog.",
    ));
    BTreeMap::from([(
        WHICH.to_owned(),
        choice(
            "A Claude Code session is blocked on a screen. Which dialog is it?",
            &options,
        ),
    )])
}

pub fn state(screen: &str) -> Value {
    json!({"screen_tail": tail_chars(screen, SCREEN_CHARS)})
}

pub fn compose(asked: &Asked) -> Option<(&'static str, f64)> {
    if asked.failed() {
        return None;
    }
    let (picked, probability) = asked.choice(WHICH)?;
    DIALOGS
        .iter()
        .find(|(key, _, _)| *key == picked)
        .map(|(_, name, _)| (*name, probability))
}

pub async fn named(decider: &Decider, screen: &str) -> Option<String> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(screen),
            questions(),
            DecisionSubject::default(),
            json!({}),
        )
        .await;
    let composed = compose(&asked);
    let score = composed.map_or(1.0, |(_, p)| p);
    let applied = if composed.is_some() && decider.applies() {
        "chosen"
    } else {
        "none"
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    composed
        .filter(|_| decider.applies())
        .map(|(name, _)| name.to_owned())
}

pub async fn dialog(decider: Option<&Decider>, screen: &str) -> Option<String> {
    if let Some(decider) = decider
        && let Some(found) = named(decider, screen).await
    {
        return Some(found);
    }
    super::blocked::dialog_name(screen)
}

#[cfg(test)]
#[path = "blocked_jev_tests.rs"]
mod tests;
