use std::path::Path;

use super::logs_collect::{
    Format, MIKE_JSON, PROXY_STDOUT, SIDECAR, Source, parse_source, text_sources,
};
use super::logs_text::Keep;

fn names(sources: &[Source]) -> Vec<(&'static str, String)> {
    sources
        .iter()
        .map(|s| {
            (
                s.label,
                s.path
                    .strip_prefix("/")
                    .unwrap_or(&s.path)
                    .display()
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn text_sources_name_the_log_dir_and_deploy_files_with_their_kinds() {
    let sources = text_sources(Some(Path::new("/logs")), Some(Path::new("/repo")));
    assert_eq!(
        names(&sources),
        [
            (SIDECAR, "logs/sidecar.log".to_owned()),
            (PROXY_STDOUT, "logs/proxy.log".to_owned()),
            (SIDECAR, "repo/.deploy/proxy-forward.log".to_owned()),
            (PROXY_STDOUT, "repo/.deploy/mike.log".to_owned()),
        ]
    );
    assert_eq!(sources[0].format, Format::Text(Keep::Levels));
    assert_eq!(sources[1].format, Format::Text(Keep::PanicsOnly));
}

#[test]
fn text_sources_without_a_checkout_skip_the_deploy_files() {
    let sources = text_sources(Some(Path::new("/logs")), None);
    assert_eq!(sources.len(), 2);
    assert!(text_sources(None, None).is_empty());
}

#[test]
fn text_sources_listing_one_file_twice_keep_it_once() {
    let sources = text_sources(Some(Path::new("/repo/.deploy")), Some(Path::new("/repo")));
    assert_eq!(sources.len(), 4);
    let same = text_sources(Some(Path::new("/repo/.deploy")), Some(Path::new("/repo")));
    assert_eq!(names(&same), names(&sources));
}

#[test]
fn parse_source_json_keeps_only_error_and_warn_lines() {
    let json = |level: &str| {
        serde_json::json!({
            "timestamp": "2026-11-17T19:30:00.100000Z",
            "level": level,
            "fields": {"message": "m"},
            "target": "mike::x",
        })
        .to_string()
    };
    let text = format!("{}\n{}\n{}", json("INFO"), json("WARN"), json("ERROR"));
    let lines = parse_source(&text, Format::Json);
    assert_eq!(lines.len(), 2);
    assert_eq!(MIKE_JSON, "mike-json");
}

#[test]
fn parse_source_proxy_stdout_keeps_only_panics() {
    let text = "2026-11-17T19:30:00.1Z  WARN mike::x: warned\nthread 'a' panicked at x.rs:1:1:";
    let lines = parse_source(text, Format::Text(Keep::PanicsOnly));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].target, "panic");
}

#[test]
fn parse_source_sidecar_keeps_warn_and_error_lines() {
    let text = "2026-11-17T19:30:00.1Z  WARN proxy_forward::ca: warned\n2026-11-17T19:30:01.1Z  INFO proxy_forward::ca: fine";
    let lines = parse_source(text, Format::Text(Keep::Levels));
    assert_eq!(lines.len(), 1);
}

fn stamp(minutes_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes_ago))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn json_line(minutes_ago: i64, level: &str, message: &str) -> String {
    serde_json::json!({
        "timestamp": stamp(minutes_ago),
        "level": level,
        "fields": {"message": message},
        "target": "mike::x",
    })
    .to_string()
}

#[tokio::test]
async fn gather_reads_the_json_log_sidecar_log_and_proxy_panics_from_both_folders()
-> anyhow::Result<()> {
    let logs = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    let deploy = root.path().join(".deploy");
    std::fs::create_dir(&deploy)?;
    let write = |path: std::path::PathBuf, body: String| std::fs::write(path, body);
    write(
        logs.path().join("mike.2026-11-17.log"),
        format!(
            "{}\n{}\n{}\n",
            json_line(5, "ERROR", "json error"),
            json_line(5, "INFO", "json info"),
            json_line(200, "ERROR", "json too old")
        ),
    )?;
    write(
        logs.path().join("sidecar.log"),
        format!("{}  WARN proxy_forward::ca: sidecar warn\n", stamp(3)),
    )?;
    write(
        logs.path().join("proxy.log"),
        format!(
            "{}  ERROR mike::x: same as json\nthread 'a' panicked at x.rs:1:1:\n",
            stamp(2)
        ),
    )?;
    write(
        deploy.join("proxy-forward.log"),
        format!("{}  ERROR proxy_forward::tls: deploy sidecar\n", stamp(1)),
    )?;
    write(
        deploy.join("mike.log"),
        format!(
            "{}  WARN mike::y: not a panic\nthread 'b' panicked at y.rs:2:2:\n",
            stamp(1)
        ),
    )?;
    let lines =
        super::logs_collect::gather(Some(logs.path()), Some(root.path()), chrono::Utc::now()).await;
    let mut seen: Vec<(String, String)> = lines
        .iter()
        .map(|l| (l.source.clone(), l.target.clone()))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            ("mike-json".to_owned(), "mike::x".to_owned()),
            ("proxy-stdout".to_owned(), "panic".to_owned()),
            ("proxy-stdout".to_owned(), "panic".to_owned()),
            ("sidecar".to_owned(), "proxy_forward::ca".to_owned()),
            ("sidecar".to_owned(), "proxy_forward::tls".to_owned()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn gather_without_any_log_files_returns_nothing() -> anyhow::Result<()> {
    let logs = tempfile::tempdir()?;
    let lines = super::logs_collect::gather(Some(logs.path()), None, chrono::Utc::now()).await;
    assert!(lines.is_empty());
    Ok(())
}
