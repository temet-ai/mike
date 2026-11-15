use super::operator_text::typed_by_operator;

fn kept(text: &str) -> Option<String> {
    typed_by_operator(text)
}

#[test]
fn typed_by_operator_plain_operator_text_survives() {
    assert_eq!(kept("commit the work").as_deref(), Some("commit the work"));
    assert_eq!(
        kept("why do we need that. we got uv").as_deref(),
        Some("why do we need that. we got uv")
    );
    assert_eq!(
        kept("  look at /tmp/x and fix it \n").as_deref(),
        Some("look at /tmp/x and fix it")
    );
    assert_eq!(kept("yes").as_deref(), Some("yes"));
}

#[test]
fn typed_by_operator_system_reminder_before_a_sentence_keeps_only_the_sentence() {
    let text = "<system-reminder>\nThe date is 2026-11-15.\n</system-reminder>\nnow deploy it";
    assert_eq!(kept(text).as_deref(), Some("now deploy it"));
}

#[test]
fn typed_by_operator_blocks_anywhere_are_removed() {
    let text = "fix the build <system-reminder>noise</system-reminder>please\n\
                <task-notification>\n<task-id>a</task-id>\n</task-notification>\n\
                <channel source=\"wake\">ping</channel>and ship\n\
                <local-command-stdout>ok</local-command-stdout>";
    assert_eq!(
        kept(text).as_deref(),
        Some("fix the build please\n\nand ship")
    );
}

#[test]
fn typed_by_operator_only_injected_blocks_is_none() {
    assert_eq!(
        kept("<system-reminder>only a reminder</system-reminder>"),
        None
    );
    assert_eq!(
        kept("<task-notification>\n<status>completed</status>\n</task-notification>"),
        None
    );
    assert_eq!(
        kept("<channel source=\"wake\" reason=\"x\">check in</channel>"),
        None
    );
    assert_eq!(
        kept(
            "<local-command-caveat>Caveat</local-command-caveat>\n<local-command-stdout>done</local-command-stdout>"
        ),
        None
    );
    assert_eq!(kept("<system-reminder>never closed"), None);
    assert_eq!(kept("   \n "), None);
}

#[test]
fn typed_by_operator_machine_prefixes_are_none() {
    for text in [
        "[Workflow harness] step 3 finished",
        "Base directory for this skill: /home/x/skills/jev",
        "<bash-input>ls</bash-input>",
        "<bash-stdout>file</bash-stdout>",
        "<command-name>/commit</command-name>",
        "<command-message>commit is running</command-message>",
        "Mail from session abc: done",
        "Another Claude session sent a message:\nhello",
        "\u{2713} built in 3.2s",
    ] {
        assert_eq!(kept(text), None, "{text}");
    }
}

#[test]
fn typed_by_operator_slash_command_alone_is_none() {
    assert_eq!(kept("/commit"), None);
    assert_eq!(kept("/review 308 --comment"), None);
    assert_eq!(kept("/jev:jev"), None);
    assert!(kept("/commit\nand then explain why").is_some());
}

#[test]
fn typed_by_operator_bare_skill_name_is_none() {
    assert_eq!(kept("rust-testing-practices"), None);
    assert_eq!(kept("fleet-worker"), None);
    assert!(kept("commit").is_some());
    assert!(kept("use rust-testing-practices").is_some());
}

#[test]
fn typed_by_operator_image_placeholder_alone_is_none() {
    assert_eq!(kept("[Image #2]"), None);
    assert_eq!(kept("[Image #1] [Image #2]"), None);
    assert_eq!(
        kept("[Image #2] this is drawn badly").as_deref(),
        Some("[Image #2] this is drawn badly")
    );
}

#[test]
fn typed_by_operator_classified_machine_text_is_none() {
    assert_eq!(kept("Tool loaded."), None);
    assert_eq!(kept("<<autonomous-loop-dynamic>>"), None);
    assert_eq!(
        kept("[SYSTEM NOTIFICATION - NOT USER INPUT]\nBackground task xyz completed."),
        None
    );
    assert_eq!(
        kept("The user stepped away and is coming back. Recap in under 40 words."),
        None
    );
}
