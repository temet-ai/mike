use super::*;

fn document(source: &'static str, name: &str, body: &str) -> FileDocument {
    let folder = if source == RULES_SOURCE {
        "rules"
    } else {
        "agents"
    };
    FileDocument {
        source,
        relative_path: format!(".claude/{folder}/{name}"),
        title: name.trim_end_matches(".md").to_owned(),
        body: body.to_owned(),
    }
}

const LESSON: &str = "Number every database migration above the newest migration on main";

#[test]
fn a_rule_that_states_the_lesson_is_shown_with_its_coverage() {
    let rows = rank(
        LESSON,
        vec![document(
            RULES_SOURCE,
            "migrations.md",
            "Every database migration is numbered above the newest migration on main.",
        )],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].relative_path, ".claude/rules/migrations.md");
    assert_eq!(rows[0].kind_label(), "rule");
    assert!(rows[0].similarity.is_some_and(|s| s >= MIN_COVERAGE));
}

#[test]
fn an_unrelated_file_is_not_shown() {
    let rows = rank(
        LESSON,
        vec![document(
            AGENTS_SOURCE,
            "ui-designer.md",
            "Maintains the design system tokens and audits the frontend for drift.",
        )],
    );
    assert!(rows.is_empty());
}

#[test]
fn at_most_two_files_are_shown_best_first() {
    let rows = rank(
        LESSON,
        vec![
            document(RULES_SOURCE, "b.md", "database migration newest main"),
            document(
                RULES_SOURCE,
                "a.md",
                "Number every database migration above the newest migration on main",
            ),
            document(RULES_SOURCE, "c.md", "database migration newest main"),
        ],
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].relative_path, ".claude/rules/a.md");
    assert_eq!(rows[1].relative_path, ".claude/rules/b.md");
}

#[test]
fn an_agent_file_is_labelled_as_an_agent_and_cited_by_its_name() {
    let rows = rank(
        "The rust-developer agent owns crates and decides on error handling",
        vec![document(
            AGENTS_SOURCE,
            "rust-developer.md",
            "The rust-developer agent owns crates and decides on error handling.",
        )],
    );
    assert_eq!(rows[0].kind_label(), "agent");
    assert!(rows[0].cited_by("covered by rust-developer"));
}

#[test]
fn an_empty_lesson_matches_nothing() {
    assert!(rank("", vec![document(RULES_SOURCE, "a.md", "anything at all")]).is_empty());
}
