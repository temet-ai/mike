use super::origin::{MOD_LESSONS_REASON, MOD_LESSONS_STAGE, label_mod_lessons, mod_lesson_ids};
use super::*;
use crate::wire::manifest::{ManifestBuilder, StageOutcome};

fn text_body(text: &str) -> Vec<u8> {
    format!(r#"{{"messages":[{{"role":"user","content":[{{"type":"text","text":"{text}"}}]}}]}}"#)
        .into_bytes()
}

#[test]
fn ids_are_read_from_the_hook_context_block_the_mod_adds() {
    let body = text_body(
        r"<system-reminder>\nLessons recorded from earlier sessions in this project:\n[lesson:12] [correction] run make fmt\n[lesson:7] [convention] name the toggle\n</system-reminder>",
    );
    assert_eq!(mod_lesson_ids(&body), vec![12, 7]);
}

#[test]
fn blocks_added_by_later_prompts_are_all_read_once_each() {
    let first = r"<system-reminder>\nLessons recorded from earlier sessions in this project:\n[lesson:1] [correction] a\n</system-reminder>";
    let second = r"<system-reminder>\nLessons recorded from earlier sessions in this project:\n[lesson:2] [correction] b\n[lesson:1] [correction] a\n</system-reminder>";
    let body = text_body(&format!("{first} and later {second}"));
    assert_eq!(mod_lesson_ids(&body), vec![1, 2]);
}

#[test]
fn a_tag_outside_the_mod_block_is_not_a_mod_lesson() {
    let body = text_body(r"notes\n[lesson:5] [correction] pasted by a person\n");
    assert!(mod_lesson_ids(&body).is_empty());
    assert_eq!(held_lesson_ids(&body), vec![5]);
}

#[test]
fn the_heading_inside_quoted_source_text_is_not_a_mod_block() {
    let body = text_body(
        r"Lessons recorded from earlier sessions in this project:\\n[lesson:5] [correction] shown in a file",
    );
    assert!(mod_lesson_ids(&body).is_empty());
}

#[test]
fn reading_stops_at_the_end_of_the_block_and_of_the_string() {
    let body = text_body(
        r"Lessons recorded from earlier sessions in this project:\n[lesson:3] [correction] a\n</system-reminder>\n[lesson:4] [correction] outside",
    );
    assert_eq!(mod_lesson_ids(&body), vec![3]);
    let ended = br#"{"a":"Lessons recorded from earlier sessions in this project:\n[lesson:3] x","b":"[lesson:4] y"}"#;
    assert_eq!(mod_lesson_ids(ended), vec![3]);
}

#[test]
fn an_escaped_quote_inside_lesson_text_does_not_end_the_block() {
    let body = text_body(
        r#"Lessons recorded from earlier sessions in this project:\n[lesson:3] [correction] say \"hi\"\n[lesson:4] [warning] b"#,
    );
    assert_eq!(mod_lesson_ids(&body), vec![3, 4]);
}

#[test]
fn ids_are_read_from_the_lines_tether_adds() {
    let body = text_body(
        r"prompt.submit hook additional context: Lesson recorded from earlier sessions in this project [lesson:2246] [correction]: finish it\n\nLesson recorded from earlier sessions in this project [lesson:9] [warning]: b",
    );
    assert_eq!(mod_lesson_ids(&body), vec![2246, 9]);
}

#[test]
fn tether_lines_and_the_mod_block_are_counted_once_together() {
    let body = text_body(
        r"Lessons recorded from earlier sessions in this project:\n[lesson:1] [correction] a\n</system-reminder> Lesson recorded from earlier sessions in this project [lesson:1] [correction]: a Lesson recorded from earlier sessions in this project [lesson:2] [warning]: b",
    );
    assert_eq!(mod_lesson_ids(&body), vec![1, 2]);
}

#[test]
fn the_tether_prefix_without_a_tag_after_it_is_not_a_lesson() {
    let body = text_body(
        r"Lesson recorded from earlier sessions in this project [lesson:N] [kind]: quoted in a doc",
    );
    assert!(mod_lesson_ids(&body).is_empty());
}

#[test]
fn a_body_without_tags_has_no_mod_lessons() {
    assert!(mod_lesson_ids(b"").is_empty());
    assert!(mod_lesson_ids(b"{}").is_empty());
    assert!(
        mod_lesson_ids(
            text_body(r"Lessons recorded from earlier sessions in this project:\n").as_slice()
        )
        .is_empty()
    );
}

#[test]
fn the_heading_and_tag_format_match_the_mod_source() {
    let source = include_str!("../../../../mods/mike/hooks/lessons.ts");
    assert!(source.contains(&format!("LESSONS_HEADING = '{LESSONS_HEADING}'")));
    assert!(source.contains(&format!("`{LESSON_TAG_OPEN}${{l.id}}]")));
}

#[test]
fn labelling_records_an_unchanged_stage_that_names_the_mod_and_the_ids() -> anyhow::Result<()> {
    let mut manifest = ManifestBuilder::new("a".repeat(64));
    let body = text_body(
        r"Lessons recorded from earlier sessions in this project:\n[lesson:12] [correction] a\n[lesson:7] [convention] b",
    );
    assert_eq!(label_mod_lessons(&body, &mut manifest), vec![12, 7]);
    let finished = manifest.finish("a".repeat(64));
    assert_eq!(finished.stages.len(), 1);
    let stage = &finished.stages[0];
    assert_eq!(stage.stage, MOD_LESSONS_STAGE);
    assert_eq!(stage.reason_code, MOD_LESSONS_REASON);
    assert_eq!(stage.outcome, StageOutcome::Unchanged);
    assert_eq!(
        stage.resolved_parameters,
        serde_json::json!({ "origin": "mod", "lesson_ids": [12, 7] })
    );
    finished.verify_chain()?;
    Ok(())
}

#[test]
fn labelling_without_ids_leaves_the_manifest_alone() {
    let mut manifest = ManifestBuilder::new("a".repeat(64));
    assert!(label_mod_lessons(b"{}", &mut manifest).is_empty());
    assert!(manifest.finish("a".repeat(64)).stages.is_empty());
}
