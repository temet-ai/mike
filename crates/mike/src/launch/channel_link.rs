use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use super::error::LaunchError;

const SCAN_LINES: usize = 200;

pub fn transcript_path(home: &Path, cwd: &Path, session: Uuid) -> PathBuf {
    home.join(".claude")
        .join("projects")
        .join(encode_cwd(cwd))
        .join(format!("{session}.jsonl"))
}

pub fn encode_cwd(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub fn read_bridge_cse_id(path: &Path) -> Result<Option<String>, LaunchError> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(LaunchError::io(
                format!("opening the transcript {}", path.display()),
                e,
            ));
        }
    };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    for _ in 0..SCAN_LINES {
        line.clear();
        let read = reader.read_line(&mut line).map_err(|e| {
            LaunchError::io(format!("reading the transcript {}", path.display()), e)
        })?;
        if read == 0 {
            break;
        }
        if let Some(id) = bridge_cse_id(&line) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub fn bridge_cse_id(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    let is_bridge = value.get("type").and_then(Value::as_str) == Some("system")
        && value.get("subtype").and_then(Value::as_str) == Some("bridge_status");
    if !is_bridge {
        return None;
    }
    let url = value.get("url").and_then(Value::as_str)?;
    let last = url.rsplit('/').next()?;
    let id = last.strip_prefix("session_")?;
    (!id.is_empty()).then(|| format!("cse_{id}"))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    const BRIDGE: &str = r#"{"parentUuid":null,"type":"system","subtype":"bridge_status","content":"/remote-control is active","url":"https://claude.ai/code/session_01SAMPLE","cwd":"/mnt/data/projects/experiments/cli","sessionId":"8793f404-7b26-4f6d-9562-58e6ccf1cf82"}"#;

    #[test]
    fn transcript_path_encodes_every_non_alphanumeric_as_dash() {
        let session: Uuid = "8793f404-7b26-4f6d-9562-58e6ccf1cf82".parse().unwrap();
        let path = transcript_path(
            Path::new("/home/vimes"),
            Path::new("/mnt/data/projects/experiments/cli"),
            session,
        );
        assert_eq!(
            path,
            PathBuf::from(
                "/home/vimes/.claude/projects/-mnt-data-projects-experiments-cli/8793f404-7b26-4f6d-9562-58e6ccf1cf82.jsonl"
            )
        );
    }

    #[test]
    fn encode_cwd_dots_and_underscores_become_dashes() {
        assert_eq!(
            encode_cwd(Path::new("/home/u/my.project_v2")),
            "-home-u-my-project-v2"
        );
    }

    #[test]
    fn bridge_cse_id_bridge_line_yields_cse_id() {
        assert_eq!(
            bridge_cse_id(BRIDGE).as_deref(),
            Some("cse_01XSQ8ZcJt6YDkhdgrcBNsXo")
        );
    }

    #[test]
    fn bridge_cse_id_other_records_and_bad_urls_are_none() {
        assert_eq!(bridge_cse_id(r#"{"type":"user","message":{}}"#), None);
        assert_eq!(
            bridge_cse_id(
                r#"{"type":"system","subtype":"bridge_status","url":"https://claude.ai/code/"}"#
            ),
            None
        );
        assert_eq!(
            bridge_cse_id(
                r#"{"type":"system","subtype":"bridge_status","url":"https://claude.ai/code/session_"}"#
            ),
            None
        );
        assert_eq!(
            bridge_cse_id(r#"{"type":"system","subtype":"bridge_status"}"#),
            None
        );
    }

    #[test]
    fn read_bridge_cse_id_missing_file_is_ok_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.jsonl");
        assert_eq!(read_bridge_cse_id(&missing).unwrap(), None);
    }

    #[test]
    fn read_bridge_cse_id_skips_malformed_lines_and_finds_the_bridge_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(
            &path,
            format!(
                "not json\n{{\"type\":\"user\"}}\n[1,2]\n{BRIDGE}\n{{\"type\":\"assistant\"}}\n"
            ),
        )
        .unwrap();
        assert_eq!(
            read_bridge_cse_id(&path).unwrap().as_deref(),
            Some("cse_01XSQ8ZcJt6YDkhdgrcBNsXo")
        );
    }

    #[test]
    fn read_bridge_cse_id_without_bridge_line_is_none_and_stops_at_the_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let mut text = "{\"type\":\"user\"}\n".repeat(SCAN_LINES);
        text.push_str(BRIDGE);
        text.push('\n');
        std::fs::write(&path, &text).unwrap();
        assert_eq!(read_bridge_cse_id(&path).unwrap(), None);
    }

    #[test]
    fn read_bridge_cse_id_unreadable_path_is_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_bridge_cse_id(dir.path()).unwrap_err();
        assert!(matches!(err, LaunchError::Io { .. }), "{err}");
    }
}
