use super::super::test_support::lesson;
use super::*;

fn with_text(id: i64, text: &str) -> LessonRow {
    LessonRow {
        text: text.to_owned(),
        ..lesson(id, Some("deploy"))
    }
}

#[test]
fn slug_lowercases_collapses_and_truncates() {
    assert_eq!(slug("Deploy & Restart!"), "deploy-restart");
    assert_eq!(slug("  "), "project");
    assert_eq!(slug(&"a".repeat(60)).len(), 40);
}

#[test]
fn triggers_ignore_urls_plain_words_and_absolute_paths() {
    let rows = [with_text(
        1,
        "See `main` and `https://x.y/z` and /abs/path/file.rs, also `make test` and tests/run.sh.",
    )];
    assert_eq!(triggers(&rows), vec!["make test", "tests/run.sh"]);
    assert!(triggers(&[with_text(2, "nothing here")]).is_empty());
}

#[test]
fn triggers_rank_by_repeats_then_first_seen_and_stop_at_three() {
    let rows = [
        with_text(
            1,
            "Never accept `cargo clippy -p` as the lint gate; run scripts/gate lint.",
        ),
        with_text(
            2,
            "Use `cargo clippy -p` only for a quick look, then `make lint`, or `x.y z`.",
        ),
    ];
    assert_eq!(
        triggers(&rows),
        vec!["cargo clippy -p", "scripts/gate", "make lint"]
    );
}

#[test]
fn topic_skill_description_names_the_work_and_the_lesson_kinds() {
    let mut fact = with_text(2, "Deploy only from the main checkout.");
    fact.kind = "fact".to_owned();
    let rows = vec![with_text(1, "Restart the proxy with start-proxy.sh."), fact];
    let body = topic_skill(
        "deploy-and-restart",
        "/work/proj",
        "deploy-and-restart",
        &rows,
    );
    assert!(body.contains("name: deploy-and-restart\n"));
    assert!(body.contains(
        "description: \"Deploy and restart: 2 accepted lessons for the `proj` project, kinds correction, fact. Use when running or touching `start-proxy.sh`, or when the task involves deploy and restart.\""
    ));
    assert!(body.contains("The lessons are in [references/lessons.md](references/lessons.md)."));
    assert!(!body.contains("## Lesson"));
    assert!(body.ends_with('\n'));
}

#[test]
fn topic_skill_for_one_agent_says_so() {
    let mut row = lesson(1, Some("deploy"));
    row.agent_id = Some(7);
    row.agent_name = Some("rust-developer".to_owned());
    let body = topic_skill("deploy-rust-developer", "/work/proj", "deploy", &[row]);
    assert!(body.contains("Applies to the rust-developer agent only.\""));
}

#[test]
fn skill_folder_carries_the_agent_only_when_every_lesson_has_the_same_one() {
    let mut first = lesson(1, Some("deploy"));
    first.agent_id = Some(7);
    first.agent_name = Some("rust-developer".to_owned());
    let mut second = first.clone();
    second.id = 2;
    assert_eq!(
        skill_folder("deploy", &[first.clone(), second.clone()]),
        "deploy-rust-developer"
    );
    second.agent_id = Some(8);
    second.agent_name = Some("db-dev".to_owned());
    assert_eq!(skill_folder("deploy", &[first, second]), "deploy");
    assert_eq!(
        skill_folder("deploy", &[lesson(3, Some("deploy"))]),
        "deploy"
    );
}

#[test]
fn lessons_reference_headings_carry_the_id_and_revision_tag() {
    let mut row = lesson(12, Some("deploy"));
    row.revision = 3;
    let body = lessons_reference("/work/proj", "deploy", &[row]);
    assert!(body.starts_with(
        "Lessons behind the deploy skill for the `proj` project, each with its source turn.\n"
    ));
    assert!(body.contains("## [lesson:12 rev:3] correction\n"));
    assert!(!body.contains("## Lesson"));
    assert!(body.contains("Source: lesson 12; turn 912; stage long_term; source operator."));
}

#[test]
fn shared_memory_headings_carry_the_tag_and_the_topic() {
    let mut row = lesson(5, Some("shared:release"));
    row.revision = 2;
    let body = shared_memory(&[row]);
    assert!(body.contains("## [lesson:5 rev:2] correction (shared:release)\n"));
    assert!(body.contains(MARKER));
}

#[test]
fn standing_body_gives_each_lesson_one_tagged_line() {
    let mut row = lesson(7, None);
    row.revision = 4;
    row.text = "Run make fmt\n\nbefore committing.".to_owned();
    let body = standing_body("/work/proj", &[&row]);
    assert!(body.starts_with(MARKER));
    assert!(body.contains("\n[lesson:7 rev:4] [correction] Run make fmt before committing.\n"));
}

#[test]
fn recipe_skill_names_never_start_with_mike() {
    let recipe = Recipe {
        id: "x".to_owned(),
        description: "d".to_owned(),
        cwd: "/c".to_owned(),
        contract: "k".to_owned(),
    };
    let (name, body) = recipe_skill(&recipe, Path::new("/repo/scripts/tool-reuse"));
    assert_eq!(name, "tool-x");
    assert!(body.contains("`/repo/scripts/tool-reuse run x`"));
    assert!(body.contains("description: \"d. Applies to /c.\""));
}
