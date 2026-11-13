use std::fmt::Write;

use crate::store::persistence::work_threads::{HistoryEntry, ThreadDetail, ThreadState};

pub fn render(detail: &ThreadDetail) -> String {
    let thread = &detail.thread;
    let mut out = String::new();
    let title = thread.title.as_deref().unwrap_or(&thread.slug);
    let _ = writeln!(out, "# {title}\n");
    write_identity(&mut out, detail);
    if let Some(goal) = &thread.goal {
        section(&mut out, "Goal", goal);
    }
    if let Some(state) = &detail.state {
        write_state(&mut out, state);
    }
    if let Some(criteria) = &detail.acceptance_criteria {
        section(&mut out, "Acceptance criteria", criteria);
    }
    write_dependencies(&mut out, detail);
    write_sessions(&mut out, detail);
    write_history(&mut out, detail);
    out
}

fn section(out: &mut String, heading: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let _ = writeln!(out, "## {heading}\n\n{}\n", text.trim());
}

fn write_identity(out: &mut String, detail: &ThreadDetail) {
    let thread = &detail.thread;
    let _ = writeln!(out, "- Thread: `{}`", thread.slug);
    let rows = [
        ("Status", thread.status.as_deref()),
        ("Branch", thread.branch.as_deref()),
        ("Worktree", thread.worktree.as_deref()),
        ("Imported from", thread.source_path.as_deref()),
        ("Parse error", thread.parse_error.as_deref()),
    ];
    for (label, value) in rows {
        if let Some(value) = value {
            let _ = writeln!(out, "- {label}: {value}");
        }
    }
    let _ = writeln!(
        out,
        "- Created: {}\n- Updated: {}\n",
        thread.created_at.format("%Y-%m-%d %H:%M UTC"),
        thread.updated_at.format("%Y-%m-%d %H:%M UTC")
    );
}

fn write_state(out: &mut String, state: &ThreadState) {
    let _ = writeln!(
        out,
        "## State of play\n\nSet {}.\n",
        state.updated_at.format("%Y-%m-%d %H:%M UTC")
    );
    let parts = [
        ("Summary", &state.tldr),
        ("Where we are", &state.where_we_are),
        ("Next", &state.next_up),
        ("Blockers", &state.blockers),
        ("Open questions", &state.open_questions),
        ("Decisions", &state.decisions),
    ];
    for (label, text) in parts {
        if !text.trim().is_empty() {
            let _ = writeln!(out, "### {label}\n\n{}\n", text.trim());
        }
    }
    if !state.files.is_empty() {
        let _ = writeln!(out, "### Files\n");
        for file in &state.files {
            let _ = writeln!(out, "- `{file}`");
        }
        out.push('\n');
    }
}

fn write_dependencies(out: &mut String, detail: &ThreadDetail) {
    let lists = [
        ("Depends on", &detail.depends_on),
        ("Needed by", &detail.depended_on_by),
    ];
    for (label, slugs) in lists {
        if slugs.is_empty() {
            continue;
        }
        let names: Vec<String> = slugs.iter().map(|slug| format!("`{slug}`")).collect();
        let _ = writeln!(out, "## {label}\n\n{}\n", names.join(", "));
    }
}

fn write_sessions(out: &mut String, detail: &ThreadDetail) {
    if detail.sessions.is_empty() {
        return;
    }
    let _ = writeln!(out, "## Sessions\n");
    for session in &detail.sessions {
        let _ = write!(out, "- `{}`", session.session_uuid);
        if !session.contribution.is_empty() {
            let _ = write!(out, " {}", session.contribution);
        }
        out.push('\n');
    }
    out.push('\n');
}

fn write_history(out: &mut String, detail: &ThreadDetail) {
    let _ = writeln!(
        out,
        "## History, newest first\n\n{} of {} entries.\n",
        detail.history.len(),
        detail.history_total
    );
    for entry in &detail.history {
        write_entry(out, entry);
    }
}

fn write_entry(out: &mut String, entry: &HistoryEntry) {
    let _ = writeln!(
        out,
        "### {} {} (entry {}): {}\n",
        entry.entry_date, entry.kind, entry.ord, entry.summary
    );
    if let Some(session) = entry.session_uuid {
        let _ = writeln!(out, "Session `{session}`");
    }
    if let Some(sha) = &entry.commit_sha {
        let _ = writeln!(out, "Commit `{sha}`");
    }
    if entry.session_uuid.is_some() || entry.commit_sha.is_some() {
        out.push('\n');
    }
    if entry.body.trim() != entry.summary.trim() {
        let _ = writeln!(out, "{}\n", entry.body.trim());
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
pub mod export_tests;
