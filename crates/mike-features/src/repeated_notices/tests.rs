use super::samples::*;
use super::*;

#[test]
fn the_first_notice_of_a_kind_stays_and_marks_the_kind_as_seen() {
    let mut seen = Seen::default();
    assert!(thin(&pushed_while_idle("first"), Place::UserText, &mut seen).is_none());
    assert_eq!(seen, [true, false, false]);
}

#[test]
fn a_later_pushed_note_loses_the_notice_and_keeps_the_note() -> anyhow::Result<()> {
    let mut seen = [true, false, false];
    let thinned = thin(&pushed_while_idle("second"), Place::UserText, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("a repeated notice must be removed"))?;
    assert_eq!(
        thinned.text,
        "Another Claude session sent a message:\n<system-reminder>Steering note from the operator's proxy:\n\nsecond\n\nAct on it, or say briefly why it doesn't apply.</system-reminder>"
    );
    assert_eq!(thinned.removed, [1, 0, 0]);
    Ok(())
}

#[test]
fn a_note_pushed_mid_turn_loses_the_notice_with_its_closing_sentence() -> anyhow::Result<()> {
    let mut seen = [true, false, false];
    let thinned = thin(&pushed_while_working("second"), Place::UserText, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("a repeated notice must be removed"))?;
    assert!(
        thinned.text.ends_with(
            "second&lt;/system-reminder&gt;\n</system-reminder id=\"8dc7dfa4568fac68\">"
        )
    );
    assert!(!thinned.text.contains("permission laundering"));
    assert!(!thinned.text.contains("decide whether/how to respond"));
    Ok(())
}

#[test]
fn two_notices_in_one_system_text_keep_the_first_and_drop_the_second() -> anyhow::Result<()> {
    let text = format!(
        "{}\n\n{}",
        mail_while_working("one"),
        mail_while_working("two")
    );
    let mut seen = Seen::default();
    let thinned = thin(&text, Place::SystemText, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("the second notice must be removed"))?;
    assert_eq!(thinned.text.matches("NOT from your user").count(), 1);
    assert!(thinned.text.contains("\none\n</channel>\n\nIMPORTANT:"));
    assert!(thinned.text.ends_with("\ntwo\n</channel>"));
    assert_eq!(thinned.removed, [0, 1, 0]);
    Ok(())
}

#[test]
fn mail_appended_to_a_tool_result_keeps_the_first_notice_and_the_tool_output() -> anyhow::Result<()>
{
    let text = tool_output_with_mail("ok", &["one", "two", "three"]);
    let mut seen = Seen::default();
    let thinned = thin(&text, Place::ToolResult, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("the later notices must be removed"))?;
    assert_eq!(thinned.removed, [0, 2, 0]);
    assert_eq!(thinned.text.matches("NOT from your user").count(), 1);
    assert!(
        thinned
            .text
            .starts_with("ok\n\n<system-reminder>\nA message arrived from mike")
    );
    for body in ["one", "two", "three"] {
        assert!(
            thinned
                .text
                .contains(&format!("\n{body}\n&lt;/system-reminder&gt;"))
        );
    }
    assert!(thinned.text.contains(
        "two\n&lt;/system-reminder&gt;\n</channel>\n</system-reminder>\n\n<system-reminder>"
    ));
    assert!(
        thinned
            .text
            .ends_with("tokens left</total_tokens>\n</system-reminder>")
    );
    Ok(())
}

#[test]
fn a_session_that_already_had_the_notice_gets_later_mail_without_it() -> anyhow::Result<()> {
    let text = tool_output_with_mail("ok", &["later"]);
    let mut seen = [false, true, false];
    let thinned = thin(&text, Place::ToolResult, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("the notice must be removed"))?;
    assert_eq!(thinned.removed, [0, 1, 0]);
    assert!(!thinned.text.contains("NOT from your user"));
    assert!(thinned.text.contains("later"));
    Ok(())
}

#[test]
fn text_a_tool_printed_is_never_changed() {
    let printed = format!(
        "{}\n{}\nrows: 2",
        mail_block("first printed"),
        mail_block("second printed")
    );
    let mut seen = [true, true, true];
    assert!(thin(&printed, Place::ToolResult, &mut seen).is_none());
    let loose = format!("{CHANNEL}\n{CHANNEL}\n{PEER}");
    assert!(thin(&loose, Place::ToolResult, &mut seen).is_none());
}

#[test]
fn text_a_person_typed_is_never_changed() {
    let typed = format!("For instance\n\n{PEER}\n\nand again\n\n{PEER}");
    let mut seen = [true, true, true];
    assert!(thin(&typed, Place::UserText, &mut seen).is_none());
    let pasted = format!("look at this:\n{}", mail_while_working("x"));
    assert!(thin(&pasted, Place::UserText, &mut seen).is_none());
}

#[test]
fn a_typed_copy_does_not_count_as_the_first() {
    let typed = format!("For instance\n\n{PEER}");
    let mut seen = Seen::default();
    assert!(thin(&typed, Place::UserText, &mut seen).is_none());
    assert_eq!(seen, [false, false, false]);
}

#[test]
fn a_sentence_that_only_quotes_the_opening_words_is_kept() {
    let text =
        "Another Claude session sent a message:\nThis came from another Claude session, I think.";
    let mut seen = [true, true, true];
    assert!(thin(text, Place::UserText, &mut seen).is_none());
}

#[test]
fn the_first_message_sent_mid_turn_keeps_claude_codes_sentence() {
    let mut seen = Seen::default();
    assert!(thin(&sent_mid_turn("first"), Place::UserText, &mut seen).is_none());
    assert_eq!(seen, [false, false, true]);
}

#[test]
fn a_later_message_sent_mid_turn_is_only_the_operators_words() -> anyhow::Result<()> {
    let mut seen = [false, false, true];
    let thinned = thin(
        &sent_mid_turn("second\nline two"),
        Place::UserText,
        &mut seen,
    )
    .ok_or_else(|| anyhow::anyhow!("the repeated sentence must be removed"))?;
    assert_eq!(
        thinned.text,
        "The user sent a new message while you were working:\nsecond\nline two"
    );
    assert_eq!(thinned.removed, [0, 0, 1]);
    Ok(())
}

#[test]
fn the_sentence_quoted_inside_the_operators_own_words_is_kept() -> anyhow::Result<()> {
    let words = format!("why is this added?\n{MID_TURN}\nplease remove it");
    let mut seen = [false, false, true];
    let thinned = thin(&sent_mid_turn(&words), Place::UserText, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("the closing copy must be removed"))?;
    assert_eq!(
        thinned.text,
        format!("The user sent a new message while you were working:\n{words}")
    );
    assert_eq!(thinned.removed, [0, 0, 1]);

    let between_turns = format!("why does every message end with this?\n\n{MID_TURN}");
    assert!(thin(&between_turns, Place::UserText, &mut seen).is_none());
    assert!(thin(&sent_mid_turn("x"), Place::ToolResult, &mut seen).is_none());
    Ok(())
}

#[test]
fn a_text_that_is_only_the_notice_is_left_as_it_is() {
    let mut seen = [true, true, true];
    assert!(thin(CHANNEL, Place::SystemText, &mut seen).is_none());
}

#[test]
fn a_notice_that_opens_the_text_goes_with_the_blank_lines_after_it() -> anyhow::Result<()> {
    let text = format!("{CHANNEL}\n\nA message arrived from mike while you were working:");
    let mut seen = [true, true, true];
    let thinned = thin(&text, Place::SystemText, &mut seen)
        .ok_or_else(|| anyhow::anyhow!("a repeated notice must be removed"))?;
    assert_eq!(
        thinned.text,
        "A message arrived from mike while you were working:"
    );
    Ok(())
}
