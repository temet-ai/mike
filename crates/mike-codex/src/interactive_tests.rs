use super::{InteractiveError, parse_interactive, proxy_args};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn local_session_keeps_caller_options_without_changing_approval_policy() -> anyhow::Result<()> {
    let input = args(&[
        "-m",
        "gpt-fixture",
        "--profile",
        "resume",
        "--sandbox=read-only",
        "fix this",
    ]);
    let parsed = parse_interactive(&input)?;
    assert_eq!(
        parsed.args,
        args(&[
            "--no-daemon",
            "-m",
            "gpt-fixture",
            "--profile",
            "resume",
            "--sandbox=read-only"
        ])
    );
    assert_eq!(parsed.model.as_deref(), Some("gpt-fixture"));
    assert_eq!(parsed.prompt.as_deref(), Some("fix this"));
    Ok(())
}

#[test]
fn explicit_cx_bypass_and_verbatim_prompt_are_preserved() -> anyhow::Result<()> {
    let input = args(&[
        "--dangerously-bypass-approvals-and-sandbox",
        "--no-daemon",
        "--",
        "resume",
    ]);
    let parsed = parse_interactive(&input)?;
    assert_eq!(
        parsed.args,
        args(&[
            "--dangerously-bypass-approvals-and-sandbox",
            "--no-daemon",
            "--"
        ])
    );
    assert_eq!(parsed.prompt.as_deref(), Some("resume"));
    Ok(())
}

#[test]
fn foreign_sessions_and_claude_flags_fail_before_launch() {
    for command in ["resume", "fork", "exec", "app-server"] {
        assert!(matches!(
            parse_interactive(&args(&["--model", "gpt-fixture", command])),
            Err(InteractiveError::NotFresh(_))
        ));
    }
    assert!(matches!(
        parse_interactive(&args(&["--remote=ws://elsewhere"])),
        Err(InteractiveError::NotFresh(_))
    ));
    assert!(matches!(
        parse_interactive(&args(&["--session-id", "fixture"])),
        Err(InteractiveError::ClaudeOption(_))
    ));
    assert!(matches!(
        parse_interactive(&args(&["--model"])),
        Err(InteractiveError::MissingValue(_))
    ));
    assert_eq!(
        parse_interactive(&args(&["first", "second"])),
        Err(InteractiveError::MultiplePrompts)
    );
}

#[test]
fn proxy_override_quotes_the_complete_url() {
    assert_eq!(
        proxy_args("http://127.0.0.1:9000/codex"),
        args(&["-c", "openai_base_url=\"http://127.0.0.1:9000/codex\""])
    );
}

#[test]
fn spawn_otel_headers_override_only_identity_with_toml_quoted_values() {
    assert_eq!(
        super::spawn_otel_args("fixture-uuid", "/home/operator/codex home"),
        args(&[
            "-c",
            "otel.exporter.otlp-http.headers.x-mike-codex-spawn-session=\"fixture-uuid\"",
            "-c",
            "otel.exporter.otlp-http.headers.x-mike-codex-home=\"/home/operator/codex home\"",
        ])
    );
}

#[test]
fn invocation_exporter_rewrites_are_refused_before_setup() -> anyhow::Result<()> {
    for input in [
        args(&["-c", "otel.exporter=\"none\""]),
        args(&["--config=otel.exporter.otlp-http.endpoint=\"https://elsewhere\""]),
        args(&["-cotel={ exporter=\"none\" }"]),
        args(&["-c", "otel.\"exporter\" = \"none\""]),
        args(&[
            "-c",
            "profiles.work.otel.exporter=\"none\"",
            "--profile",
            "work",
        ]),
        args(&["-c", "profiles.work.otel={ exporter=\"none\" }"]),
        args(&["-c", "profiles.work={ otel={ exporter=\"none\" } }"]),
        args(&["-c", "profiles={ work={ otel={ exporter=\"none\" } } }"]),
    ] {
        assert!(matches!(
            parse_interactive(&input),
            Err(InteractiveError::TelemetryOverride(_))
        ));
    }
    assert!(
        parse_interactive(&args(&["-c", "otel.trace_exporter=\"none\"", "hello"]))?
            .prompt
            .is_some()
    );
    assert!(
        parse_interactive(&args(&[
            "--profile",
            "work",
            "-c",
            "profiles.work.model=\"gpt-fixture\"",
            "hello"
        ]))?
        .prompt
        .is_some()
    );
    Ok(())
}

#[test]
fn compact_options_preserve_their_value_and_cannot_retarget_the_working_directory()
-> anyhow::Result<()> {
    let parsed = parse_interactive(&args(&[
        "-mgpt-fixture",
        "-cmodel_reasoning_effort=\"high\"",
        "hello",
    ]))?;
    assert_eq!(parsed.model.as_deref(), Some("gpt-fixture"));
    assert_eq!(
        parsed.args,
        args(&[
            "--no-daemon",
            "-mgpt-fixture",
            "-cmodel_reasoning_effort=\"high\""
        ])
    );
    assert!(matches!(
        parse_interactive(&args(&["-C/another-project"])),
        Err(InteractiveError::NotFresh(_))
    ));
    assert!(matches!(
        parse_interactive(&args(&["--model", "--no-daemon"])),
        Err(InteractiveError::MissingValue(_))
    ));
    Ok(())
}
