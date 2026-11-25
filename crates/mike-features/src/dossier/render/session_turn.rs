use super::cut::{char_count, cut, cut_front};
use crate::store::persistence::dossier_view::RecordTurnRow;

pub const STOP_CAP: usize = 3_000;
pub const SHORT_STOP_CAP: usize = 600;
pub const MIN_ENDING_STOP_CAP: usize = 200;
const SHORT_PROMPT_CAP: usize = 200;
const MACHINE_PROMPT_CAP: usize = 300;
const TOOLS_LISTED: usize = 10;
const COMMAND_CAP: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Full,
    Short,
}

impl Detail {
    pub fn stop_cap(self) -> usize {
        match self {
            Detail::Full => STOP_CAP,
            Detail::Short => SHORT_STOP_CAP,
        }
    }
}

pub struct TurnStyle<'a> {
    pub number: usize,
    pub detail: Detail,
    pub stop_cap: usize,
    pub budget: usize,
    pub reaction: Option<&'a str>,
}

pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn is_operator(turn: &RecordTurnRow) -> bool {
    match turn.prompt_kind.as_deref() {
        Some(kind) => kind == "genuine",
        None => matches!(turn.opened_by.as_deref(), None | Some("operator")),
    }
}

pub fn kind_label(turn: &RecordTurnRow) -> String {
    match turn.prompt_kind.as_deref() {
        Some("genuine") => "operator".to_owned(),
        Some("background-event" | "task-notification") => "background notification".to_owned(),
        Some("loop-wakeup" | "channel-wake") => "wakeup".to_owned(),
        Some("peer-message") => "peer message".to_owned(),
        Some(other) => other.replace('-', " "),
        None => turn
            .opened_by
            .clone()
            .filter(|by| by != "operator")
            .unwrap_or_else(|| "operator".to_owned()),
    }
}

fn prompt_text(turn: &RecordTurnRow, style: &TurnStyle<'_>) -> String {
    let machine_cap = match style.detail {
        Detail::Full => MACHINE_PROMPT_CAP,
        Detail::Short => SHORT_PROMPT_CAP,
    };
    if !is_operator(turn) {
        return cut(&collapse(&turn.prompt), machine_cap);
    }
    if style.detail == Detail::Short {
        return cut(&collapse(&turn.prompt), SHORT_PROMPT_CAP);
    }
    let cap = (style.budget / 4).max(1);
    if char_count(&turn.prompt) > cap {
        format!(
            "{} (prompt_chars_total={})",
            cut(&turn.prompt, cap),
            char_count(&turn.prompt)
        )
    } else {
        turn.prompt.clone()
    }
}

fn stopped_with(turn: &RecordTurnRow, cap: usize) -> String {
    let text = |t: &str| cut_front(t, cap);
    let stop = turn.stop_text.as_deref().filter(|t| !t.trim().is_empty());
    let last = turn.last_text.as_deref().filter(|t| !t.trim().is_empty());
    let body = match (stop, last) {
        (Some(stop), _) => text(stop),
        (None, Some(last)) => format!(
            "(the last request wrote no text; last text in the turn:)\n{}",
            text(last)
        ),
        (None, None) => format!(
            "(no text: stop_reason={}, aborted={}, interrupted={})",
            turn.stop_reason.as_deref().unwrap_or("unknown"),
            turn.aborted.unwrap_or(false),
            turn.interrupted.unwrap_or(false)
        ),
    };
    format!("STOPPED WITH:\n{body}")
}

fn checks_part(turn: &RecordTurnRow) -> String {
    match turn.checks_count {
        Some(run) => format!("checks failed {} of {run}", turn.checks_failed.unwrap_or(0)),
        None => "checks not recorded".to_owned(),
    }
}

fn tools_part(turn: &RecordTurnRow) -> String {
    let listed: Vec<String> = turn
        .tools()
        .take(TOOLS_LISTED)
        .map(|(name, n)| format!("{name} x{n}"))
        .collect();
    let more = turn.tool_names.len().saturating_sub(TOOLS_LISTED);
    match (listed.is_empty(), more) {
        (true, _) => "tools none".to_owned(),
        (false, 0) => format!("tools {}", listed.join(" ")),
        (false, more) => format!("tools {} (+{more} more)", listed.join(" ")),
    }
}

fn did_short(turn: &RecordTurnRow) -> String {
    format!(
        "DID: files {}, commits {}, pushes {}, errors {}, {}",
        turn.files_touched,
        turn.commit_count,
        turn.push_count,
        turn.tool_errors,
        checks_part(turn)
    )
}

fn did_full(turn: &RecordTurnRow) -> String {
    let mut out = format!(
        "DID: files {}, commits {}, pushes {}, {}, errors {}, {}, stop_reason={}, ended_on={}, aborted={}, interrupted={}",
        turn.files_touched,
        turn.commit_count,
        turn.push_count,
        tools_part(turn),
        turn.tool_errors,
        checks_part(turn),
        turn.stop_reason.as_deref().unwrap_or("unknown"),
        turn.ended_on.as_deref().unwrap_or("none"),
        turn.aborted.unwrap_or(false),
        turn.interrupted.unwrap_or(false),
    );
    for (label, commands) in [
        ("commit", &turn.commit_commands),
        ("push", &turn.push_commands),
    ] {
        for command in commands {
            out.push_str(&format!(
                "\n  {label}: {}",
                cut(&collapse(command), COMMAND_CAP)
            ));
        }
    }
    out
}

pub fn render_turn(turn: &RecordTurnRow, style: &TurnStyle<'_>) -> String {
    let did = match style.detail {
        Detail::Full => did_full(turn),
        Detail::Short => did_short(turn),
    };
    let mut out = format!(
        "#{} {} {}\nPROMPT:\n{}\n{}\n{did}",
        style.number,
        turn.opened_at.format("%H:%M"),
        kind_label(turn),
        prompt_text(turn, style),
        stopped_with(turn, style.stop_cap),
    );
    if let Some(reaction) = style.reaction {
        out.push_str(&format!("\nREACTION: {reaction}"));
    }
    out
}
