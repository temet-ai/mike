use chrono::Utc;

use super::input_v2::{
    INPUT_BUILDER_V2, INPUT_BUILDER_V2_0, JudgeTurnContext, build_judge_input_v2,
};

fn ctx(text: String, cut: i32) -> JudgeTurnContext {
    JudgeTurnContext {
        turn_id: 1,
        session_id: 1,
        user_prompt: "fix the parser".to_owned(),
        final_assistant_text: text,
        text_cut_at: cut,
        tool_call_count: 3,
        tool_error_count: 1,
        next_genuine_request_id: Some(9),
        next_genuine_text: Some("thanks".to_owned()),
        next_genuine_at: None,
        closed_at: Utc::now(),
    }
}

fn numbered(n: usize) -> String {
    (0..n)
        .map(|i| char::from(b'a' + u8::try_from(i % 26).unwrap_or(0)))
        .collect()
}

#[test]
fn the_trimmed_builder_cuts_at_text_cut_at_before_taking_the_tail() {
    let text = numbered(10_000);
    let input = build_judge_input_v2(&ctx(text.clone(), 6_000), INPUT_BUILDER_V2);
    let expected: String = text.chars().skip(3_000).take(3_000).collect();
    assert!(input.text.contains(&format!(
        "cut at the front when long):\n{expected}\n\nTOOL USE"
    )));
    let untrimmed = build_judge_input_v2(&ctx(text.clone(), 6_000), INPUT_BUILDER_V2_0);
    let tail: String = text.chars().skip(7_000).collect();
    assert!(untrimmed.text.contains(&tail));
}

#[test]
fn no_next_message_renders_none() {
    let mut c = ctx("done".to_owned(), 4);
    c.next_genuine_text = None;
    let input = build_judge_input_v2(&c, INPUT_BUILDER_V2);
    assert!(
        input
            .text
            .ends_with("OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n(none)")
    );
    assert_eq!(input.as_of, c.closed_at);
}

#[derive(serde::Deserialize)]
struct EInputRow {
    judge_prompt: String,
    judge_assistant: String,
    tool_call_count: i32,
    tool_error_count: i32,
    next_genuine_for_judge: String,
}

#[test]
fn the_base_fields_of_the_e_input_rows_render_byte_equal_to_w4() -> anyhow::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rel = "plans/judgement-context/p0/e-input.2026-09-24.json";
    let Some(text) = [
        root.join("../../artifacts").join(rel),
        root.join("../../../../night-watch-notes").join(rel),
    ]
    .iter()
    .find_map(|path| std::fs::read_to_string(path).ok()) else {
        return Ok(());
    };
    let rows: Vec<EInputRow> = serde_json::from_str(&text)?;
    for row in &rows {
        let c = JudgeTurnContext {
            user_prompt: row.judge_prompt.clone(),
            final_assistant_text: row.judge_assistant.clone(),
            text_cut_at: i32::try_from(row.judge_assistant.chars().count()).unwrap_or(i32::MAX),
            tool_call_count: row.tool_call_count,
            tool_error_count: row.tool_error_count,
            next_genuine_text: Some(row.next_genuine_for_judge.clone()),
            ..ctx(String::new(), 0)
        };
        let expected = format!(
            "OPERATOR REQUEST (user prompt):\n{}\n\nASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):\n{}\n\nTOOL USE SUMMARY:\ntool_call_count = {}\ntool_error_count = {}\n\nOPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n{}",
            row.judge_prompt,
            row.judge_assistant,
            row.tool_call_count,
            row.tool_error_count,
            row.next_genuine_for_judge
        );
        assert_eq!(build_judge_input_v2(&c, INPUT_BUILDER_V2_0).text, expected);
    }
    Ok(())
}
