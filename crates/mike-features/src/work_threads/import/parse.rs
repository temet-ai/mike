use chrono::NaiveDate;
use uuid::Uuid;

use super::RegistryEntry;
use super::frontmatter::{self, Frontmatter, as_list, strip_quotes};
use super::paths::{first_token, is_none_like, looks_like_path, relativize};
use super::sections::{self, StateOfPlay, all_uuids, first_uuid};
use crate::store::persistence::work_threads::{
    ALLOWED_STATUSES, ImportedEntry, ImportedState, ImportedThread,
};

const TLDR_CHARS: usize = 600;
const LEGACY_STATUSES: [(&str, &str); 2] = [("complete", "done"), ("in-progress", "implementing")];

pub struct ParseContext<'a> {
    pub slug: &'a str,
    pub source_path: &'a str,
    pub roots: &'a [String],
    pub registry: Option<&'a RegistryEntry>,
    pub file_date: NaiveDate,
}

pub struct Parsed {
    pub thread: ImportedThread,
    pub depends_on: Vec<String>,
}

fn first_chars(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

fn without_thread_label(title: &str) -> String {
    let title = title.trim();
    match title.get(..7) {
        Some(head) if head.eq_ignore_ascii_case("thread:") => title[7..].trim().to_owned(),
        _ => title.to_owned(),
    }
}

fn heading_title(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty())
}

fn normalise_status(raw: Option<&str>, notes: &mut Vec<String>) -> Option<String> {
    let raw = raw.map(|value| strip_quotes(value).to_ascii_lowercase())?;
    if raw.is_empty() {
        return None;
    }
    if ALLOWED_STATUSES.contains(&raw.as_str()) {
        return Some(raw);
    }
    if let Some((_, mapped)) = LEGACY_STATUSES.iter().find(|(legacy, _)| *legacy == raw) {
        notes.push(format!("status `{raw}` is stored as `{mapped}`"));
        return Some((*mapped).to_owned());
    }
    notes.push(format!(
        "status `{raw}` is not a known status; stored as none"
    ));
    None
}

fn branch_of(candidates: &[Option<&str>]) -> Option<String> {
    candidates
        .iter()
        .flatten()
        .find_map(|value| first_token(value))
}

fn worktree_of(
    candidates: &[Option<&str>],
    roots: &[String],
    notes: &mut Vec<String>,
) -> Option<String> {
    let token = candidates
        .iter()
        .flatten()
        .find_map(|value| first_token(value))?;
    if !looks_like_path(&token) {
        notes.push(format!("worktree `{token}` is not a path; stored as none"));
        return None;
    }
    match relativize(&token, roots) {
        Some(relative) if relative.is_empty() => None,
        Some(relative) => Some(relative),
        None => {
            notes.push(format!(
                "worktree `{token}` is outside every known location of the project; stored as none"
            ));
            None
        }
    }
}

fn session_ids(front: &Frontmatter) -> (Option<Uuid>, Vec<Uuid>) {
    let listed: Vec<Uuid> = front.get("session_id").map(all_uuids).unwrap_or_default();
    let last = front
        .get("last_session")
        .and_then(first_uuid)
        .or_else(|| listed.last().copied());
    let mut all = listed;
    if let Some(last) = last
        && !all.contains(&last)
    {
        all.push(last);
    }
    (last, all)
}

fn imported_state(
    state: Option<&StateOfPlay>,
    goal: Option<&str>,
    roots: &[String],
) -> Option<ImportedState> {
    let where_we_are = state.map(|s| s.where_we_are.clone()).unwrap_or_default();
    let tldr_source = if where_we_are.is_empty() {
        goal.unwrap_or_default()
    } else {
        &where_we_are
    };
    let imported = ImportedState {
        tldr: first_chars(tldr_source, TLDR_CHARS),
        next_up: state.map(|s| s.next_up.clone()).unwrap_or_default(),
        blockers: state.map(|s| s.blockers.clone()).unwrap_or_default(),
        open_questions: state.map(|s| s.open_questions.clone()).unwrap_or_default(),
        decisions: state.map(|s| s.decisions.clone()).unwrap_or_default(),
        files: state
            .map(|s| sections::file_list(&s.files, roots))
            .unwrap_or_default(),
        where_we_are,
    };
    let empty = imported.tldr.is_empty()
        && imported.next_up.is_empty()
        && imported.blockers.is_empty()
        && imported.open_questions.is_empty()
        && imported.decisions.is_empty()
        && imported.files.is_empty();
    (!empty).then_some(imported)
}

fn entries(body: &str, ctx: &ParseContext<'_>, notes: &mut Vec<String>) -> Vec<ImportedEntry> {
    sections::session_log(body)
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let date = entry.date.unwrap_or_else(|| {
                notes.push(format!(
                    "session log entry {} has no readable date; the file's date was used",
                    index + 1
                ));
                ctx.file_date
            });
            ImportedEntry {
                date,
                session_uuid: entry.session,
                commit_sha: entry.sha,
                summary: entry.summary,
                body: entry.body,
            }
        })
        .collect()
}

fn dependency_slugs(front: &Frontmatter, state: Option<&StateOfPlay>) -> Vec<String> {
    let mut slugs: Vec<String> = front
        .first_of(&["dependencies", "depends_on"])
        .map(as_list)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| entry.split_whitespace().next().map(str::to_owned))
        .filter(|slug| !is_none_like(slug))
        .collect();
    if let Some(state) = state {
        slugs.extend(sections::dependency_slugs(&state.dependencies));
    }
    slugs.sort();
    slugs.dedup();
    slugs
}

pub fn parse_worklog(ctx: &ParseContext<'_>, text: &str) -> Parsed {
    let split = frontmatter::split(text);
    let front = &split.frontmatter;
    let body = split.body;
    let mut notes = Vec::new();
    let mut errors: Vec<String> = split.error.into_iter().collect();
    if text.trim().is_empty() {
        errors.push("the worklog file is empty".to_owned());
    }
    let state_of_play = sections::state_of_play(body);
    let mut title = front
        .get("title")
        .map(|title| strip_quotes(title).to_owned())
        .or_else(|| heading_title(body))
        .map(|title| without_thread_label(&title))
        .filter(|title| !title.is_empty());
    if title.is_none()
        && let Some(named) = front
            .get("thread")
            .map(|name| strip_quotes(name).to_owned())
    {
        notes.push(format!(
            "the worklog has no title; its `thread` name `{named}` is used"
        ));
        title = Some(named);
    }
    if title.is_none() {
        errors.push("no title in the frontmatter or a first-level heading".to_owned());
    }
    let goal = front
        .first_of(&["goal", "summary"])
        .map(|g| g.trim().to_owned());
    let registry = ctx.registry;
    let status = normalise_status(
        front
            .get("status")
            .or(registry.and_then(|r| r.status.as_deref())),
        &mut notes,
    );
    let branch = branch_of(&[
        front.get("branch"),
        state_of_play.as_ref().map(|s| s.branch.as_str()),
        registry.and_then(|r| r.branch.as_deref()),
    ]);
    let worktree = worktree_of(
        &[
            front.get("worktree"),
            state_of_play.as_ref().map(|s| s.worktree.as_str()),
            registry.and_then(|r| r.worktree.as_deref()),
        ],
        ctx.roots,
        &mut notes,
    );
    let (last_session_uuid, session_uuids) = session_ids(front);
    let thread = ImportedThread {
        slug: ctx.slug.to_owned(),
        title,
        status,
        branch,
        worktree,
        last_session_uuid,
        source_path: ctx.source_path.to_owned(),
        parse_error: (!errors.is_empty()).then(|| errors.join("; ")),
        state: imported_state(state_of_play.as_ref(), goal.as_deref(), ctx.roots),
        acceptance_criteria: sections::criteria(body),
        entries: entries(body, ctx, &mut notes),
        original_text: text.to_owned(),
        original_date: ctx.file_date,
        session_uuids,
        notes,
        goal,
    };
    Parsed {
        depends_on: dependency_slugs(front, state_of_play.as_ref()),
        thread,
    }
}

pub fn registry_stub(
    ctx: &ParseContext<'_>,
    entry: Option<&RegistryEntry>,
    read_error: Option<String>,
) -> Parsed {
    let mut notes = Vec::new();
    let reason = read_error.unwrap_or_else(|| {
        "the thread is in the registry but no worklog.md was found under artifacts/threads/ or docs/threads/ of its repository".to_owned()
    });
    let thread = ImportedThread {
        slug: ctx.slug.to_owned(),
        title: None,
        goal: None,
        status: normalise_status(entry.and_then(|e| e.status.as_deref()), &mut notes),
        branch: branch_of(&[entry.and_then(|e| e.branch.as_deref())]),
        worktree: worktree_of(
            &[entry.and_then(|e| e.worktree.as_deref())],
            ctx.roots,
            &mut notes,
        ),
        last_session_uuid: None,
        source_path: ctx.source_path.to_owned(),
        parse_error: Some(reason),
        state: None,
        acceptance_criteria: None,
        entries: Vec::new(),
        original_text: entry.map(|e| e.raw.clone()).unwrap_or_default(),
        original_date: ctx.file_date,
        session_uuids: Vec::new(),
        notes,
    };
    Parsed {
        thread,
        depends_on: Vec::new(),
    }
}
