#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use crate::launch::terminal::TerminalTimeouts;

pub(in crate::launch::terminal) fn fast_timeouts() -> TerminalTimeouts {
    TerminalTimeouts {
        window: Duration::from_millis(300),
        window_poll: Duration::from_millis(20),
        child_pid: Duration::from_millis(150),
        marker: Duration::from_millis(300),
        self_close: Duration::from_millis(1200),
        forced_close: Duration::from_millis(300),
    }
}

const XDG_SCRIPT: &str = r#"#!/bin/sh
DIR="$(dirname "$0")"
{ for a in "$@"; do printf '%s\037' "$a"; done; printf '\n'; } >> "$DIR/argv.log"
app_id=""
for a in "$@"; do
  case "$a" in
    --print-id)
      if [ -f "$DIR/print_id.fail" ]; then cat "$DIR/print_id.fail" >&2; exit 1; fi
      if [ -f "$DIR/print_id.out" ]; then cat "$DIR/print_id.out"; exit 0; fi
      printf 'foot.desktop\n'
      exit 0
      ;;
    --app-id=*) app_id="${a#--app-id=}" ;;
  esac
done
while [ "$1" != "--" ] && [ $# -gt 0 ]; do shift; done
shift
echo "$$ $app_id" >> "$DIR/windows.log"
"$@" &
wait
"#;

const HYPR_SCRIPT: &str = r#"#!/bin/sh
DIR="$(dirname "$0")"
{ for a in "$@"; do printf '%s\037' "$a"; done; printf '\n'; } >> "$DIR/argv.log"
case "$1" in
  clients)
    if [ -f "$DIR/clients.empty" ]; then printf '[]'; exit 0; fi
    printf '['
    first=1
    if [ -f "$DIR/windows.log" ]; then
      while read -r pid app_id; do
        if kill -0 "$pid" 2>/dev/null; then
          [ "$first" -eq 0 ] && printf ','
          first=0
          printf '{"pid":%s,"class":"%s"}' "$pid" "$app_id"
        fi
      done < "$DIR/windows.log"
    fi
    printf ']'
    exit 0
    ;;
  version)
    if [ -f "$DIR/version.fail" ]; then cat "$DIR/version.fail" >&2; exit 1; fi
    if [ -f "$DIR/version.json" ]; then cat "$DIR/version.json"; exit 0; fi
    printf '{"version":"0.56.2"}'
    exit 0
    ;;
  *)
    printf "unknown command %s\n" "$1" >&2
    exit 1
    ;;
esac
"#;

pub(in crate::launch::terminal) struct FakeTerminalHost {
    dir: tempfile::TempDir,
}

impl FakeTerminalHost {
    pub(in crate::launch::terminal) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write_script(&dir.path().join("xdg-terminal-exec"), XDG_SCRIPT);
        write_script(&dir.path().join("hyprctl"), HYPR_SCRIPT);
        Self { dir }
    }

    pub(in crate::launch::terminal) fn xdg_bin(&self) -> PathBuf {
        self.dir.path().join("xdg-terminal-exec")
    }

    pub(in crate::launch::terminal) fn hypr_bin(&self) -> PathBuf {
        self.dir.path().join("hyprctl")
    }

    pub(in crate::launch::terminal) fn fail_print_id(&self, message: &str) -> &Self {
        std::fs::write(self.dir.path().join("print_id.fail"), message).unwrap();
        self
    }

    pub(in crate::launch::terminal) fn fail_version(&self, message: &str) -> &Self {
        std::fs::write(self.dir.path().join("version.fail"), message).unwrap();
        self
    }

    pub(in crate::launch::terminal) fn make_clients_report_no_windows(&self) -> &Self {
        std::fs::write(self.dir.path().join("clients.empty"), "").unwrap();
        self
    }

    pub(in crate::launch::terminal) fn windows(&self) -> Vec<(u32, String)> {
        let log = std::fs::read_to_string(self.dir.path().join("windows.log")).unwrap_or_default();
        log.lines()
            .filter_map(|line| {
                let (pid, app_id) = line.split_once(' ')?;
                Some((pid.parse().ok()?, app_id.to_string()))
            })
            .collect()
    }
}

fn write_script(path: &std::path::Path, script: &str) {
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
