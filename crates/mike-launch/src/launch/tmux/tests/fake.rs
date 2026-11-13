#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SCRIPT: &str = r#"#!/bin/sh
DIR="$(dirname "$0")"
{ for a in "$@"; do printf '%s\037' "$a"; done; printf '\n'; } >> "$DIR/argv.log"
key="$1"
k=$(printf '%s' "$key" | tr -c 'A-Za-z0-9' '_')
d="$DIR/answers/$k"
if [ ! -d "$d" ]; then
  printf "tmux: unknown command: %s\n" "$key" >&2
  exit 1
fi
n=$(cat "$d/count" 2>/dev/null || echo 0)
echo $((n+1)) > "$d/count"
f="$d/$n"
[ -e "$f.code" ] || f="$d/last"
cat "$f.out"
cat "$f.err" >&2
exit "$(cat "$f.code")"
"#;

pub struct FakeTmux {
    dir: tempfile::TempDir,
}

impl FakeTmux {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("tmux");
        std::fs::write(&bin, SCRIPT).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    pub fn bin(&self) -> PathBuf {
        self.dir.path().join("tmux")
    }

    pub fn answer(&self, key: &str, out: &str) -> &Self {
        self.push(key, out, "", 0)
    }

    pub fn error(&self, key: &str, stderr: &str) -> &Self {
        self.push(key, "", stderr, 1)
    }

    fn push(&self, key: &str, stdout: &str, stderr: &str, code: i32) -> &Self {
        let dir = self.dir.path().join("answers").join(sanitise(key));
        std::fs::create_dir_all(&dir).unwrap();
        let n = std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                let name = e.as_ref().unwrap().file_name();
                let name = name.to_string_lossy();
                name.ends_with(".code") && !name.starts_with("last")
            })
            .count();
        for stem in [n.to_string(), "last".to_string()] {
            let base = dir.join(stem);
            write(&base, "out", stdout);
            write(&base, "err", stderr);
            write(&base, "code", &code.to_string());
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_answers_in_queue_order_repeats_last_and_logs_argv() {
        let fake = FakeTmux::new();
        fake.answer("-V", "tmux 3.7c\n");
        fake.answer("list-panes", "%1\n")
            .answer("list-panes", "%2\n");
        fake.error("kill-session", "can't find session: nope");
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
            String::from_utf8_lossy(&run(&["-V"]).await.stdout),
            "tmux 3.7c\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&run(&["list-panes"]).await.stdout),
            "%1\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&run(&["list-panes"]).await.stdout),
            "%2\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&run(&["list-panes"]).await.stdout),
            "%2\n"
        );
        let killed = run(&["kill-session", "-t", "nope"]).await;
        assert_eq!(killed.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&killed.stderr).contains("can't find session"));
        assert_eq!(fake.calls().len(), 5);
        assert_eq!(fake.calls()[4], vec!["kill-session", "-t", "nope"]);
    }
}
