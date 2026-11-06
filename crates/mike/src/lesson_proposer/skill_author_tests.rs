use super::*;
use crate::store::persistence::lessons::skill_live::{RetiredNumbers, RetiredReason};

fn target() -> DraftTarget {
    DraftTarget {
        project_id: 7,
        project_path: "/mnt/data/projects/temet/temet-proxy".to_owned(),
        folder: "database-migrations".to_owned(),
        topic: "database-migrations".to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 790,
    }
}

fn lesson(id: i64, kind: &str, text: &str, prompt: Option<&str>) -> DraftLesson {
    DraftLesson {
        id,
        kind: kind.to_owned(),
        text: text.to_owned(),
        user_prompt: prompt.map(str::to_owned),
    }
}

fn reference(name: &str) -> AuthoredReference {
    AuthoredReference {
        name: name.to_owned(),
        when_to_read: "you are adding or renumbering a migration.".to_owned(),
        content: "Run `mike migrate-check` before committing.\n\nLessons: 316.".to_owned(),
    }
}

fn authored(yes: usize, no: usize) -> AuthoredSkill {
    let mut trigger_queries = Vec::new();
    for i in 0..yes {
        trigger_queries.push(TriggerQuery {
            query: format!("should {i}"),
            should_trigger: true,
        });
    }
    for i in 0..no {
        trigger_queries.push(TriggerQuery {
            query: format!("not {i}"),
            should_trigger: false,
        });
    }
    AuthoredSkill {
        description: "Ship a migration safely. Use when adding or changing a migration.".to_owned(),
        overview: "Migrations run on proxy start.\n\n\
                   - [references/numbering.md](references/numbering.md): read when numbering one.\n\
                   - [references/committing.md](references/committing.md): read when committing one."
            .to_owned(),
        references: vec![reference("numbering"), reference("committing")],
        trigger_queries,
    }
}

fn rejected(skill: &AuthoredSkill, needle: &str) {
    match acceptable(skill) {
        Err(err) => assert!(err.contains(needle), "{err}"),
        Ok(()) => panic!("expected a rejection naming {needle}"),
    }
}

#[test]
fn build_input_names_project_topic_lessons_and_prompts() {
    let input = build_input(
        &target(),
        &[
            lesson(
                316,
                "warning",
                "Run migrate-check first.",
                Some("add a migration please"),
            ),
            lesson(
                360,
                "correction",
                "Commit migrations with their code.",
                None,
            ),
        ],
        None,
        None,
    );
    assert!(input.starts_with("## PROJECT\ntemet-proxy\n"));
    assert!(input.contains("## TOPIC\ndatabase-migrations\n"));
    assert!(input.contains("[316] (warning) Run migrate-check first."));
    assert!(input.contains("[360] (correction) Commit migrations with their code."));
    assert!(input.contains("## PROMPTS THAT PRODUCED THESE LESSONS\n- add a migration please"));
    assert!(!input.contains("## AGENT"));
}

#[test]
fn build_input_names_the_agent_when_scoped() {
    let mut t = target();
    t.agent_name = Some("fleet-coordinator".to_owned());
    let input = build_input(
        &t,
        &[
            lesson(1, "warning", "x", None),
            lesson(2, "warning", "y", None),
        ],
        None,
        None,
    );
    assert!(input.contains("applies only to the fleet-coordinator agent"));
}

fn rejected_draft() -> PreviousDraft {
    PreviousDraft {
        revision: 2,
        lesson_watermark: 790,
        description: "Old description about migrations.".to_owned(),
        skill_md: "---\nname: database-migrations\n---\nold body".to_owned(),
        reference_files: json!([{
            "name": "numbering",
            "when_to_read": "numbering one.",
            "content": "# Numbering\n\nUse the next free timestamp."
        }]),
        eval: Some(json!({
            "verdict": {"accepted": false, "reasons": ["the patched draft's trigger score 0.55 is below 0.80"]},
            "review": {
                "decision": "patch",
                "reason": "The description never names renumbering.",
                "patched_skill_md": "---\nname: database-migrations\n---\npatched body"
            },
            "findings": {
                "missed_triggers": [{"query": "renumber my sqlx migration", "loaded": ["ui-design"]}],
                "wrong_triggers": [{"query": "explain postgres vacuum", "loaded": ["database-migrations"]}]
            }
        })),
    }
}

fn retired_skill(reason: RetiredReason, shipped: Option<PreviousDraft>) -> RetiredSkill {
    let day = |d: u32| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap_or_default();
    let at = |d: u32| day(d).and_hms_opt(12, 0, 0).unwrap_or_default().and_utc();
    RetiredSkill {
        numbers: RetiredNumbers {
            first_shipped_on: day(1),
            window_start: at(1),
            window_end: at(14),
            loads: 12,
            loaded_turns: 9,
            judged_turns: 8,
            corrected_turns: 4,
            base_judged_turns: 40,
            base_corrected_turns: 8,
            retired_at: at(14),
            reason,
        },
        shipped,
    }
}

#[test]
fn build_input_carries_a_rejected_drafts_evaluation_into_the_next_draft() {
    let input = build_input(
        &target(),
        &[
            lesson(1, "warning", "x", None),
            lesson(2, "warning", "y", None),
        ],
        Some(&rejected_draft()),
        None,
    );
    assert!(input.contains("## PREVIOUS DRAFT (REJECTED)"));
    assert!(input.contains("Old description about migrations."));
    assert!(input.contains("### references/numbering.md\n# Numbering"));
    assert!(
        input.contains("Rejected because the patched draft's trigger score 0.55 is below 0.80.")
    );
    assert!(input.contains("\"renumber my sqlx migration\" (the model loaded: ui-design)"));
    assert!(input.contains("\"explain postgres vacuum\""));
    assert!(
        input.contains(
            "The reviewer's decision was patch: The description never names renumbering."
        )
    );
    assert!(input.contains("patched body"));
}

#[test]
fn reference_named_with_its_path_or_md_suffix_is_accepted_under_its_bare_name() {
    let mut skill = authored(8, 8);
    skill.references[0].name = "numbering.md".to_owned();
    skill.references[1].name = "references/committing.md".to_owned();
    assert!(acceptable(&skill).is_err());
    shape::normalise_reference_names(&mut skill);
    assert_eq!(skill.references[0].name, "numbering");
    assert_eq!(skill.references[1].name, "committing");
    assert!(acceptable(&skill).is_ok());
}

#[test]
fn a_description_over_the_skill_limit_is_rejected() {
    let mut skill = authored(8, 8);
    assert!(acceptable(&skill).is_ok());
    skill.description = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
    rejected(&skill, "description is 1025 characters, limit 1024");
    skill.description = "x".repeat(MAX_DESCRIPTION_CHARS);
    assert!(acceptable(&skill).is_ok());
}

#[test]
fn the_author_prompt_covers_review_and_question_phrasings_in_description_and_queries() {
    assert!(AUTHOR_PROMPT.contains("Cover reviewing, auditing and checking work"));
    assert!(AUTHOR_PROMPT.contains("answering questions about it"));
    assert!(AUTHOR_PROMPT.contains("under 1,024 characters"));
    assert!(AUTHOR_PROMPT.contains("at least a third as reviews, audits, checks or questions"));
    assert!(AUTHOR_PROMPT.contains("review the migrations added this week"));
    assert!(AUTHOR_PROMPT.contains("which toggles start enabled"));
}

#[test]
fn build_input_without_a_rejected_draft_has_no_previous_section() {
    let input = build_input(&target(), &[lesson(1, "warning", "x", None)], None, None);
    assert!(!input.contains("PREVIOUS DRAFT"));
    assert!(!input.contains("RETIRED SKILL"));
}

#[test]
fn build_input_carries_a_retired_skills_live_numbers_and_its_text() {
    let shipped = rejected_draft();
    let retired = retired_skill(RetiredReason::CorrectedAboveBase, Some(shipped));
    let input = build_input(
        &target(),
        &[lesson(1, "warning", "x", None)],
        None,
        Some(&retired),
    );
    assert!(input.contains("## RETIRED SKILL"));
    assert!(input.contains("corrected more often"));
    assert!(input.contains("loaded 12 times in 9 operator turns"));
    assert!(input.contains("corrected 50% (4 of 8)"));
    assert!(input.contains("corrected rate was 20% (8 of 40)"));
    assert!(input.contains("### Its SKILL.md\n---\nname: database-migrations"));
    assert!(input.contains("### references/numbering.md\n# Numbering"));
}

#[test]
fn a_skill_retired_for_never_being_loaded_says_so_and_may_have_no_authored_text() {
    let retired = retired_skill(RetiredReason::Unloaded, None);
    let input = build_input(
        &target(),
        &[lesson(1, "warning", "x", None)],
        None,
        Some(&retired),
    );
    assert!(input.contains("No session loaded it for 30 days"));
    assert!(input.contains("loaded 12 times"));
    assert!(input.contains("without an authored draft"));
    assert!(!input.contains("### Its SKILL.md"));
}

#[test]
fn a_retirement_with_no_judged_turn_reports_no_rate_instead_of_dividing_by_zero() {
    let mut retired = retired_skill(RetiredReason::Unloaded, None);
    retired.numbers.judged_turns = 0;
    retired.numbers.corrected_turns = 0;
    retired.numbers.base_judged_turns = 0;
    retired.numbers.base_corrected_turns = 0;
    let input = build_input(
        &target(),
        &[lesson(1, "warning", "x", None)],
        None,
        Some(&retired),
    );
    assert!(input.contains("not measurable (no judged turn)"));
}

#[test]
fn build_input_says_so_when_a_rejected_draft_has_no_findings() {
    let mut previous = rejected_draft();
    previous.eval = None;
    let input = build_input(
        &target(),
        &[lesson(1, "warning", "x", None)],
        Some(&previous),
        None,
    );
    assert!(input.contains("No findings were recorded."));
}

#[test]
fn skill_md_has_frontmatter_marker_title_and_the_overview_only() {
    let skill = authored(8, 8);
    let md = skill_md(
        "database-migrations",
        "Ship a migration safely.",
        &skill.overview,
    );
    assert!(md.starts_with(
        "---\nname: database-migrations\ndescription: \"Ship a migration safely.\"\n---\n"
    ));
    assert!(md.contains(MARKER));
    assert!(md.contains("# Database migrations\n"));
    assert!(md.contains("references/numbering.md"));
    assert!(!md.contains("migrate-check"));
}

#[test]
fn reference_md_titles_the_file_and_says_when_to_read_it() {
    let md = reference_md(&reference("numbering"));
    assert!(md.starts_with(
        "# Numbering\n\nRead this when you are adding or renumbering a migration.\n\n"
    ));
    assert!(md.contains("Lessons: 316."));
}

#[test]
fn reference_files_carry_name_and_rendered_content() {
    let files = reference_files(&authored(8, 8));
    assert_eq!(files[0]["name"], "numbering");
    assert_eq!(files[1]["name"], "committing");
    assert!(
        files[0]["content"]
            .as_str()
            .is_some_and(|c| c.starts_with("# Numbering"))
    );
}

#[test]
fn acceptable_takes_a_short_overview_with_a_pointer_to_every_reference() {
    assert!(acceptable(&authored(8, 8)).is_ok());
}

#[test]
fn acceptable_rejects_an_overview_missing_a_pointer() {
    let mut skill = authored(8, 8);
    skill.overview = "- [references/numbering.md](references/numbering.md): read when.".to_owned();
    rejected(&skill, "references/committing.md");
}

#[test]
fn acceptable_rejects_an_overview_over_the_limit() {
    let mut skill = authored(8, 8);
    skill.overview = format!(
        "{} references/numbering.md references/committing.md",
        "x".repeat(MAX_OVERVIEW_CHARS)
    );
    rejected(&skill, "overview is");
}

#[test]
fn acceptable_rejects_bad_missing_reserved_or_repeated_reference_names() {
    let mut skill = authored(8, 8);
    skill.references = Vec::new();
    rejected(&skill, "reference files");
    for name in ["lessons", "Numbering", "../x", "a--b", ""] {
        let mut skill = authored(8, 8);
        skill.references[0].name = name.to_owned();
        rejected(&skill, "is not allowed");
    }
    let mut twice = authored(8, 8);
    twice.references[1].name = "numbering".to_owned();
    rejected(&twice, "named twice");
    let mut many = authored(8, 8);
    many.references = (0..=MAX_REFERENCES)
        .map(|i| reference(&format!("r{i}")))
        .collect();
    rejected(&many, "reference files");
}

#[test]
fn acceptable_rejects_a_reference_over_the_limit() {
    let mut skill = authored(8, 8);
    skill.references[0].content = "x".repeat(MAX_REFERENCE_CHARS + 1);
    rejected(&skill, "limit");
}

#[test]
fn acceptable_requires_balanced_trigger_queries() {
    rejected(&authored(3, 8), "trigger queries");
    rejected(&authored(8, 2), "trigger queries");
    let mut blank = authored(8, 8);
    blank.description = "  ".to_owned();
    rejected(&blank, "empty description");
}

#[test]
fn schema_requires_every_field() {
    let s = schema();
    assert_eq!(
        s["required"],
        serde_json::json!(["description", "overview", "references", "trigger_queries"])
    );
    assert_eq!(
        s["properties"]["references"]["items"]["required"],
        serde_json::json!(["name", "when_to_read", "content"])
    );
}
