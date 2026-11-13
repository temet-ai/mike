#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::json;

const SCRIPT: &str = r#"#!/bin/sh
DIR="$(dirname "$0")"
{ for a in "$@"; do printf '%s\037' "$a"; done; printf '\n'; } >> "$DIR/argv.log"
key="$1${2:+ $2}"
k=$(printf '%s' "$key" | tr -c 'A-Za-z0-9' '_')
d="$DIR/answers/$k"
if [ ! -d "$d" ]; then
  printf '%s' "{\"error\":{\"code\":\"no_canned_answer\",\"message\":\"$key\"},\"id\":\"fake\"}" >&2
  exit 1
fi
n=$(cat "$d/count" 2>/dev/null || echo 0)
echo $((n+1)) > "$d/count"
f="$d/$n"
[ -e "$f.code" ] || f="$d/last"
[ -e "$f.hang" ] && sleep 30
cat "$f.out"
cat "$f.err" >&2
exit "$(cat "$f.code")"
"#;

pub struct FakeHerdr {
    dir: tempfile::TempDir,
}

impl FakeHerdr {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("herdr");
        std::fs::write(&bin, SCRIPT).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    pub fn bin(&self) -> PathBuf {
        self.dir.path().join("herdr")
    }

    pub fn answer(&self, key: &str, stdout: &str) -> &Self {
        self.push(key, stdout, "", 0, false)
    }

    pub fn error(&self, key: &str, code: &str, message: &str) -> &Self {
        let stderr = json!({"error": {"code": code, "message": message}, "id": "fake"}).to_string();
        self.push(key, "", &stderr, 1, false)
    }

    pub fn hang(&self, key: &str) -> &Self {
        self.push(key, "", "", 0, true)
    }

    fn push(&self, key: &str, stdout: &str, stderr: &str, code: i32, hang: bool) -> &Self {
        let dir = self.dir.path().join("answers").join(sanitise(key));
        std::fs::create_dir_all(&dir).unwrap();
        let n = std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".code")
                    && !e
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("last")
            })
            .count();
        for stem in [n.to_string(), "last".to_string()] {
            let base = dir.join(stem);
            write(&base, "out", stdout);
            write(&base, "err", stderr);
            write(&base, "code", &code.to_string());
            let hang_marker = base.with_extension("hang");
            if hang {
                std::fs::write(&hang_marker, "").unwrap();
            } else if hang_marker.exists() {
                std::fs::remove_file(&hang_marker).unwrap();
            }
        }
        self
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        let log = std::fs::read_to_string(self.dir.path().join("argv.log")).unwrap_or_default();
        log.lines()
            .map(|line| {
                line.split('\u{1f}')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }
}

fn write(base: &Path, ext: &str, content: &str) {
    std::fs::write(base.with_extension(ext), content).unwrap();
}

fn sanitise(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_answers_in_queue_order_repeats_last_and_logs_argv() {
        let fake = FakeHerdr::new();
        fake.answer("agent list", "first")
            .answer("agent list", "second");
        fake.error("pane close", "pane_not_found", "gone");
        let run = |args: &[&str]| {
            let bin = fake.bin();
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            async move {
                tokio::process::Command::new(bin)
                    .args(&args)
                    .output()
                    .await
                    .unwrap()
            }
        };
        assert_eq!(
            String::from_utf8_lossy(&run(&["agent", "list"]).await.stdout),
            "first"
        );
        assert_eq!(
            String::from_utf8_lossy(&run(&["agent", "list"]).await.stdout),
            "second"
        );
        assert_eq!(
            String::from_utf8_lossy(&run(&["agent", "list"]).await.stdout),
            "second"
        );
        let closed = run(&["pane", "close", "w1:p1"]).await;
        assert_eq!(closed.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&closed.stderr).contains("pane_not_found"));
        let unknown = run(&["tab", "list"]).await;
        assert_eq!(unknown.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&unknown.stderr).contains("no_canned_answer"));
        let calls = fake.calls();
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[3], vec!["pane", "close", "w1:p1"]);
    }
}
