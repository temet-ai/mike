use std::path::PathBuf;

use chrono::NaiveDate;
use uuid::Uuid;

use super::frontmatter;
use super::parse::{ParseContext, Parsed, parse_worklog, registry_stub};
use super::paths::{is_none_like, relativize};
use super::registry::{self, RegistryEntry};
use super::sources::find_worklogs;

const BRANCH_CONSOLIDATION: &str = include_str!(
    "../../../tests/fixtures/threads/repo/artifacts/threads/branch-consolidation/worklog.md"
);
const FRONTEND_MOTION: &str = include_str!(
    "../../../tests/fixtures/threads/repo/artifacts/threads/frontend-motion/worklog.md"
);
const JUDGEMENT_CONTEXT: &str = include_str!(
    "../../../tests/fixtures/threads/repo/artifacts/threads/judgement-context/worklog.md"
);
const NO_TITLE: &str =
    include_str!("../../../tests/fixtures/threads/repo/artifacts/threads/no-title/worklog.md");

fn fixture_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/threads/repo")
}

fn roots() -> Vec<String> {
    vec![
        "/Users/vimes/projects/temet/temet-proxy".to_owned(),
        "/mnt/data/projects/temet/temet-proxy".to_owned(),
    ]
}

fn file_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 21).unwrap_or_default()
}

fn context<'a>(
    slug: &'a str,
    roots: &'a [String],
    registry: Option<&'a RegistryEntry>,
) -> ParseContext<'a> {
    ParseContext {
        slug,
        source_path: "/repo/artifacts/threads/x/worklog.md",
        roots,
        registry,
        file_date: file_date(),
    }
}

fn uuid(text: &str) -> anyhow::Result<Uuid> {
    Ok(Uuid::parse_str(text)?)
}

fn parsed(slug: &str, text: &str) -> Parsed {
    let roots = roots();
    parse_worklog(&context(slug, &roots, None), text)
}

#[test]
fn a_worklog_with_plain_labels_gives_identity_and_state() -> anyhow::Result<()> {
    let parsed = parsed("frontend-motion", FRONTEND_MOTION);
    let thread = &parsed.thread;
    assert_eq!(thread.title.as_deref(), Some("frontend-motion"));
    assert_eq!(thread.status.as_deref(), Some("implementing"));
    assert_eq!(thread.branch.as_deref(), Some("main"));
    assert_eq!(thread.worktree.as_deref(), Some(".claude/worktrees/motion"));
    assert_eq!(thread.parse_error, None);
    assert_eq!(
        thread.last_session_uuid,
        Some(uuid("47933f70-bbb7-486f-84ec-4dd8109b0c4d")?)
    );
    assert!(
        thread
            .goal
            .as_deref()
            .is_some_and(|g| g.starts_with("Two angles"))
    );
    let state = thread
        .state
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no state"))?;
    assert!(state.where_we_are.starts_with("Angle A is applied"));
    assert!(state.where_we_are.contains("session pages"));
    assert_eq!(state.tldr, state.where_we_are);
    assert!(state.next_up.starts_with("(1) Finish A4"));
    assert_eq!(state.blockers, "usage budget.");
    assert_eq!(state.open_questions, "");
    assert_eq!(thread.original_text, FRONTEND_MOTION);
    Ok(())
}

#[test]
fn a_worklog_with_plain_labels_gives_files_dependencies_and_criteria() -> anyhow::Result<()> {
    let parsed = parsed("frontend-motion", FRONTEND_MOTION);
    let thread = &parsed.thread;
    let state = thread
        .state
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no state"))?;
    assert_eq!(
        state.files,
        [
            "frontend/src/lib/components/ui/motion.css",
            "frontend/src/lib/components/ui/motion.ts"
        ]
    );
    assert_eq!(
        parsed.depends_on,
        ["branch-consolidation", "judgement-context"]
    );
    assert!(
        thread
            .acceptance_criteria
            .as_deref()
            .is_some_and(|c| c.starts_with("A1."))
    );
    Ok(())
}

#[test]
fn both_session_log_sections_of_a_worklog_are_read() -> anyhow::Result<()> {
    let parsed = parsed("frontend-motion", FRONTEND_MOTION);
    let thread = &parsed.thread;
    assert_eq!(thread.entries.len(), 3);
    assert_eq!(thread.entries[0].summary, "thread created.");
    assert_eq!(thread.entries[0].commit_sha.as_deref(), Some("e4a99823"));
    assert_eq!(
        thread.entries[0].session_uuid,
        Some(uuid("47933f70-bbb7-486f-84ec-4dd8109b0c4d")?)
    );
    assert_eq!(
        thread.entries[2].date,
        NaiveDate::from_ymd_opt(2026, 9, 9).unwrap_or_default()
    );
    assert_eq!(thread.entries[2].commit_sha, None);
    Ok(())
}

#[test]
fn a_worklog_with_bulleted_labels_gives_identity_and_state() -> anyhow::Result<()> {
    let parsed = parsed("branch-consolidation", BRANCH_CONSOLIDATION);
    let thread = &parsed.thread;
    assert_eq!(thread.title.as_deref(), Some("Branch consolidation fleet"));
    assert_eq!(thread.status.as_deref(), Some("done"));
    assert_eq!(thread.branch.as_deref(), Some("main"));
    assert_eq!(thread.worktree, None);
    assert!(
        thread.notes.iter().any(|n| n.contains("not a path")),
        "{:?}",
        thread.notes
    );
    let state = thread
        .state
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no state"))?;
    assert!(state.where_we_are.starts_with("DONE. All 13 branches"));
    assert!(state.where_we_are.contains("Full gate green"));
    assert_eq!(state.next_up, "nothing — thread complete.");
    assert_eq!(state.blockers, "");
    assert!(state.decisions.contains("scope all 13"));
    assert!(state.files.is_empty());
    assert!(parsed.depends_on.is_empty());
    assert!(
        thread
            .acceptance_criteria
            .as_deref()
            .is_some_and(|c| c.contains("AC2"))
    );
    Ok(())
}

#[test]
fn heading_dated_session_log_entries_keep_their_own_text() -> anyhow::Result<()> {
    let parsed = parsed("branch-consolidation", BRANCH_CONSOLIDATION);
    let thread = &parsed.thread;
    assert_eq!(thread.entries.len(), 2);
    assert_eq!(thread.entries[0].summary, "coordinator (726c5233)");
    assert_eq!(
        thread.entries[0].date,
        NaiveDate::from_ymd_opt(2026, 7, 28).unwrap_or_default()
    );
    assert!(thread.entries[0].body.contains("Phase 1: operator settled"));
    assert!(!thread.entries[0].body.contains("Landed W8"));
    assert_eq!(
        thread.last_session_uuid,
        Some(uuid("726c5233-7a56-4e89-9aed-647eddeea1fd")?)
    );
    Ok(())
}

#[test]
fn an_unlabelled_state_of_play_is_kept_whole_and_the_session_list_is_read() -> anyhow::Result<()> {
    let roots = roots();
    let parsed = parse_worklog(
        &context("judgement-context", &roots, None),
        JUDGEMENT_CONTEXT,
    );
    let thread = &parsed.thread;
    assert_eq!(
        thread.title.as_deref(),
        Some("Judgement-context fleet worklog")
    );
    assert_eq!(thread.session_uuids.len(), 2);
    assert_eq!(
        thread.last_session_uuid,
        Some(uuid("a1b2c3d4-0000-4000-8000-000000000002")?)
    );
    let state = thread
        .state
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no state"))?;
    assert!(state.where_we_are.contains("RUN STOPPED"));
    assert!(state.where_we_are.contains("Not started: JC03"));
    assert!(state.tldr.chars().count() <= 600);
    assert!(thread.entries.is_empty());
    assert_eq!(thread.acceptance_criteria, None);
    assert_eq!(thread.branch, None);
    Ok(())
}

#[test]
fn a_file_with_nothing_recognisable_gets_a_parse_error_and_keeps_its_text() {
    let roots = roots();
    let parsed = parse_worklog(&context("no-title", &roots, None), NO_TITLE);
    let thread = &parsed.thread;
    assert_eq!(thread.title, None);
    assert!(
        thread
            .parse_error
            .as_deref()
            .is_some_and(|e| e.contains("no title"))
    );
    assert!(thread.state.is_none());
    assert_eq!(thread.original_text, NO_TITLE);
    assert!(thread.entries.is_empty());
}

#[test]
fn unclosed_frontmatter_is_reported_and_the_heading_still_gives_a_title() {
    let text = "---\nstatus: active\n\n# A heading title\n\nbody\n";
    let roots = roots();
    let parsed = parse_worklog(&context("open", &roots, None), text);
    assert_eq!(parsed.thread.title.as_deref(), Some("A heading title"));
    assert!(
        parsed
            .thread
            .parse_error
            .as_deref()
            .is_some_and(|e| e.contains("never closes"))
    );
    let empty = parse_worklog(&context("blank", &roots, None), "  \n");
    assert!(
        empty
            .thread
            .parse_error
            .as_deref()
            .is_some_and(|e| e.contains("empty"))
    );
}

#[test]
fn registry_values_fill_what_the_file_leaves_out_and_the_file_wins_elsewhere() {
    let entry = RegistryEntry {
        thread: "judgement-context".into(),
        repo: "/Users/vimes/projects/temet/temet-proxy".into(),
        status: Some("parked".into()),
        branch: Some("feat/x".into()),
        worktree: Some("/Users/vimes/projects/temet/temet-proxy/platform/x".into()),
        raw: String::new(),
    };
    let roots = roots();
    let parsed = parse_worklog(
        &context("judgement-context", &roots, Some(&entry)),
        JUDGEMENT_CONTEXT,
    );
    assert_eq!(parsed.thread.status.as_deref(), Some("active"));
    assert_eq!(parsed.thread.branch.as_deref(), Some("feat/x"));
    assert_eq!(parsed.thread.worktree.as_deref(), Some("platform/x"));
}

#[test]
fn an_unknown_status_is_stored_as_none_with_a_note() {
    let text = "---\ntitle: T\nstatus: finished\n---\n";
    let roots = roots();
    let parsed = parse_worklog(&context("odd", &roots, None), text);
    assert_eq!(parsed.thread.status, None);
    assert!(parsed.thread.notes.iter().any(|n| n.contains("finished")));
}

#[test]
fn a_registry_entry_without_a_file_becomes_a_row_with_the_reason() {
    let entry = RegistryEntry {
        thread: "missing-from-disk".into(),
        repo: "/tmp/mike-thread-fixture/repo".into(),
        status: Some("review".into()),
        branch: Some("feat/x".into()),
        worktree: Some("/tmp/mike-thread-fixture/repo/platform/x".into()),
        raw: "{\"thread\": \"missing-from-disk\"}".into(),
    };
    let roots = vec!["/tmp/mike-thread-fixture/repo".to_owned()];
    let parsed = registry_stub(
        &context("missing-from-disk", &roots, Some(&entry)),
        Some(&entry),
        None,
    );
    let thread = &parsed.thread;
    assert_eq!(thread.title, None);
    assert_eq!(thread.status.as_deref(), Some("review"));
    assert_eq!(thread.worktree.as_deref(), Some("platform/x"));
    assert!(
        thread
            .parse_error
            .as_deref()
            .is_some_and(|e| e.contains("registry"))
    );
    assert_eq!(thread.original_text, entry.raw);
}

#[test]
fn worklogs_are_found_in_slug_order_and_the_registry_reports_its_bad_lines() -> anyhow::Result<()> {
    let slugs: Vec<String> = find_worklogs(&fixture_repo())
        .into_iter()
        .map(|f| f.slug)
        .collect();
    assert_eq!(
        slugs,
        [
            "branch-consolidation",
            "frontend-motion",
            "judgement-context",
            "no-title"
        ]
    );
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/threads/registry.jsonl");
    let read = registry::read(&path)?;
    assert_eq!(read.entries.len(), 2);
    assert_eq!(
        read.entries[1].worktree.as_deref(),
        Some("/tmp/mike-thread-fixture/repo/platform/x")
    );
    assert_eq!(read.problems.len(), 2);
    assert!(
        read.problems[0].ends_with(":3 is not JSON"),
        "{:?}",
        read.problems
    );
    assert!(registry::read(&path.with_file_name("absent.jsonl")).is_err());
    Ok(())
}

#[test]
fn paths_under_a_root_become_relative_and_the_rest_is_refused() {
    let roots = roots();
    assert_eq!(
        relativize("/mnt/data/projects/temet/temet-proxy/a/b.rs", &roots).as_deref(),
        Some("a/b.rs")
    );
    assert_eq!(
        relativize("/Users/vimes/projects/temet/temet-proxy", &roots).as_deref(),
        Some("")
    );
    assert_eq!(
        relativize("./docs/x.md", &roots).as_deref(),
        Some("docs/x.md")
    );
    assert_eq!(relativize("/elsewhere/file", &roots), None);
    assert_eq!(relativize("../up/file", &roots), None);
    assert_eq!(
        relativize("/mnt/data/projects/temet/temet-proxy-other/x", &roots),
        None
    );
}

#[test]
fn the_many_spellings_of_none_are_recognised() {
    for value in [
        "null",
        "none",
        "(none)",
        "(none — in-repo)",
        "~",
        "None.",
        "",
        "`none`",
        "n/a",
    ] {
        assert!(is_none_like(value), "{value:?}");
    }
    for value in ["main", "nothing — thread complete.", "feat/none-of-it"] {
        assert!(!is_none_like(value), "{value:?}");
    }
}

#[test]
fn frontmatter_values_continue_on_indented_lines_and_lists_parse() {
    let split = frontmatter::split(
        "---\nsummary: >\n  first line\n  second line\nsession_id:\n  - a\n  - b\ntags: [x, \"y\"]\n---\nbody",
    );
    assert_eq!(split.body, "body");
    assert_eq!(
        split.frontmatter.get("summary"),
        Some("first line\nsecond line")
    );
    assert_eq!(
        frontmatter::as_list(split.frontmatter.get("session_id").unwrap_or_default()),
        ["a", "b"]
    );
    assert_eq!(
        frontmatter::as_list(split.frontmatter.get("tags").unwrap_or_default()),
        ["x", "y"]
    );
}

#[test]
fn a_worklog_without_a_title_uses_its_thread_name_and_says_so() {
    let text = "---\nthread: grype-pipelines\nstatus: active\ndependencies: none\nbranch: none\n---\n\n## State of play\nWhere we are: scanning.\n";
    let parsed = parsed("grype-pipelines", text);
    assert_eq!(parsed.thread.title.as_deref(), Some("grype-pipelines"));
    assert_eq!(parsed.thread.parse_error, None);
    assert!(parsed.thread.notes.iter().any(|n| n.contains("no title")));
    assert_eq!(parsed.thread.branch, None);
    assert!(parsed.depends_on.is_empty());
}

#[test]
fn a_title_written_as_thread_colon_slug_drops_the_label() {
    let text = "---\nthread: spawn-into-mike\ntitle: \"Thread: spawn-into-mike\"\nstatus: active\n---\n\n# Thread: ignored heading\n";
    let labelled = parsed("spawn-into-mike", text);
    assert_eq!(labelled.thread.title.as_deref(), Some("spawn-into-mike"));
    let heading = parsed(
        "anti-false",
        "---\nthread: anti-false\nstatus: active\n---\n\n# thread: Anti false claims\n",
    );
    assert_eq!(heading.thread.title.as_deref(), Some("Anti false claims"));
}

#[test]
fn a_frontmatter_dependency_with_a_note_keeps_only_its_slug() {
    let text = "---\nthread: fleet-and-docs\ntitle: Fleet and docs\nstatus: done\ndepends_on: spawn-into-mike (steps 1-5, all landed on main as of ec745dd4)\n---\n\n## State of play\nWhere we are: done.\n";
    let parsed = parsed("fleet-and-docs", text);
    assert_eq!(parsed.depends_on, vec!["spawn-into-mike".to_owned()]);
}
