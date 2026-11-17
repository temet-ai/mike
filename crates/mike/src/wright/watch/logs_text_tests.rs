use chrono::{DateTime, Utc};

use super::logs_text::{Keep, parse_text, strip_ansi};

fn at(stamp: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(stamp.parse()?)
}

#[test]
fn parse_text_reads_a_warn_line_with_a_target_and_trailing_fields() -> anyhow::Result<()> {
    let text = "2026-11-07T09:55:14.582251Z  WARN proxy_forward::ca: generated a NEW MITM CA — re-trust it (proxy-forward export-ca) before Claude Code will accept the intercepted TLS dir=/x";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].level, "WARN");
    assert_eq!(lines[0].target, "proxy_forward::ca");
    assert!(lines[0].message.starts_with("generated a NEW MITM CA"));
    assert_eq!(lines[0].at, at("2026-11-07T09:55:14.582251Z")?);
    Ok(())
}

#[test]
fn parse_text_keeps_a_colon_inside_the_message() {
    let text = "2026-11-17T19:17:32.143283Z  WARN proxy_forward::connect: CONNECT → blind tunnel to an OpenAI model host: Codex traffic goes direct";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].target, "proxy_forward::connect");
    assert_eq!(
        lines[0].message,
        "CONNECT → blind tunnel to an OpenAI model host: Codex traffic goes direct"
    );
}

#[test]
fn parse_text_skips_span_prefixes_before_the_target() {
    let text = "2026-11-17T19:30:00.1Z  ERROR request{id=5}: mike_features::proxy::response::streaming: weasel intercept stands aside";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].level, "ERROR");
    assert_eq!(lines[0].target, "mike_features::proxy::response::streaming");
    assert_eq!(lines[0].message, "weasel intercept stands aside");
}

#[test]
fn parse_text_drops_levels_below_warn() {
    let text = "2026-11-17T19:30:00.1Z  INFO request{id=5}: mike_features::proxy::response::streaming: weasel intercept stands aside";
    assert!(parse_text(text, Keep::Levels).is_empty());
}

#[test]
fn parse_text_reads_an_ansi_coloured_line_like_a_plain_one() {
    let text = "\u{1b}[2m2026-11-17T19:17:32.143283Z\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \u{1b}[2mproxy_forward::connect\u{1b}[0m\u{1b}[2m:\u{1b}[0m CONNECT → blind tunnel";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].level, "WARN");
    assert_eq!(lines[0].target, "proxy_forward::connect");
    assert_eq!(lines[0].message, "CONNECT → blind tunnel");
    assert!(!lines[0].raw.contains('\u{1b}'));
}

#[test]
fn strip_ansi_removes_colour_codes_only() {
    assert_eq!(strip_ansi("\u{1b}[2;33mwarn\u{1b}[0m x"), "warn x");
}

#[test]
fn parse_text_gives_a_panic_line_the_previous_timestamp() -> anyhow::Result<()> {
    let text = "2026-11-17T19:30:00.1Z  INFO mike::serve: started\nthread 'tokio-runtime-worker' panicked at crates/x.rs:10:5:\nboom";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].level, "ERROR");
    assert_eq!(lines[0].target, "panic");
    assert!(lines[0].message.contains("panicked at crates/x.rs:10:5"));
    assert_eq!(lines[0].at, at("2026-11-17T19:30:00.1Z")?);
    Ok(())
}

#[test]
fn parse_text_skips_a_panic_before_any_timestamp() {
    let text =
        "thread 'main' panicked at crates/x.rs:1:1:\n2026-11-17T19:30:00.1Z  WARN mike::x: later";
    let lines = parse_text(text, Keep::Levels);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].target, "mike::x");
}

#[test]
fn parse_text_panics_only_keeps_nothing_but_panics() {
    let text = "2026-11-17T19:30:00.1Z  ERROR mike::x: same event as the json log\nthread 'main' panicked at crates/x.rs:1:1:";
    let lines = parse_text(text, Keep::PanicsOnly);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].target, "panic");
}
