use super::classify::{self, OrphanKind};
use super::runner::ProcRunner;
use crate::launch::error::LaunchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub ppid: u32,
    pub uptime: String,
    pub cwd: String,
    pub session: Option<String>,
    pub model: Option<String>,
    pub host: String,
    pub orphan: OrphanKind,
}

const PS_ARGS: [&str; 2] = ["-eo", "pid=,ppid=,etime=,args="];

pub async fn scan(runner: &dyn ProcRunner) -> Result<Vec<ClaudeProcess>, LaunchError> {
    let output = runner.run("ps", &PS_ARGS.map(str::to_string)).await;
    if !output.ok() {
        return Err(LaunchError::Failed(format!(
            "procs: scan: ps: {}",
            output
                .exec_error
                .unwrap_or_else(|| format!("exited {}", output.exit_code))
        )));
    }
    let mut out = Vec::new();
    for line in output.stdout.lines() {
        let Some((pid, ppid, etime, rest)) = split_ps_line(line) else {
            continue;
        };
        if !is_claude_command(rest) {
            continue;
        }
        let (host, orphan) = match infra_label(rest) {
            Some(label) => (label.to_string(), OrphanKind::NotOrphan),
            None => classify::classify_host(runner, pid).await,
        };
        out.push(ClaudeProcess {
            pid,
            ppid,
            uptime: fmt_uptime(etime),
            cwd: pid_cwd(pid),
            session: parse_flag_value(rest, "--session-id"),
            model: parse_flag_value(rest, "--model"),
            host,
            orphan,
        });
    }
    Ok(out)
}

fn split_ps_line(line: &str) -> Option<(u32, u32, &str, &str)> {
    let mut rest = line.trim_start();
    let mut fields: Vec<&str> = Vec::with_capacity(3);
    while fields.len() < 3 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    let pid: u32 = fields[0].parse().ok()?;
    let ppid: u32 = fields[1].parse().ok()?;
    let etime = fields[2];
    Some((pid, ppid, etime, rest.trim_start()))
}

fn is_claude_command(rest: &str) -> bool {
    let first = rest.split_whitespace().next().unwrap_or("");
    let basename = first.rsplit('/').next().unwrap_or(first);
    basename == "claude" || rest.contains("share/claude/versions/")
}

const INFRA_SUBCOMMANDS: [&str; 3] = ["daemon", "bg-pty-host", "bg-spare"];
const INFRA_FLAGS: [&str; 1] = ["--chrome-native-host"];

fn infra_label(rest: &str) -> Option<&'static str> {
    let mut tokens = rest.split_whitespace();
    let subcommand = tokens.nth(1);
    if let Some(sub) = subcommand
        && let Some(&matched) = INFRA_SUBCOMMANDS.iter().find(|&&s| s == sub)
    {
        return Some(match matched {
            "daemon" => "claude infrastructure (daemon)",
            "bg-pty-host" => "claude infrastructure (bg-pty-host)",
            _ => "claude infrastructure (bg-spare)",
        });
    }
    rest.split_whitespace()
        .any(|t| INFRA_FLAGS.contains(&t))
        .then_some("claude infrastructure (chrome-native-host)")
}

fn parse_flag_value(rest: &str, flag: &str) -> Option<String> {
    let mut tokens = rest.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == flag {
            return tokens.next().map(str::to_string);
        }
        if let Some(value) = token.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value.to_string());
        }
    }
    None
}

fn fmt_uptime(etime: &str) -> String {
    let (days, rest) = match etime.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().unwrap_or(0), r),
        None => (0, etime),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (hours, minutes) = match parts.as_slice() {
        [h, m, _s] => (h.parse().unwrap_or(0), m.parse().unwrap_or(0)),
        [m, _s] => (0u64, m.parse().unwrap_or(0)),
        _ => (0, 0),
    };
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        "<1m".to_string()
    }
}

fn pid_cwd(pid: u32) -> String {
    crate::launch::process::cwd(pid)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::super::runner::ProcRunOutput;
    use super::*;

    #[test]
    fn ps_args_are_separate_tokens_with_no_embedded_whitespace() {
        assert_eq!(PS_ARGS.len(), 2);
        for token in PS_ARGS {
            assert!(
                !token.contains(' '),
                "{token:?} must be its own argv element"
            );
        }
    }

    #[test]
    fn split_ps_line_reads_pid_ppid_etime_and_keeps_rest_whitespace() {
        let (pid, ppid, etime, rest) =
            split_ps_line("  1234   1  04:12:33  /usr/bin/claude --session-id abc  --foo").unwrap();
        assert_eq!(pid, 1234);
        assert_eq!(ppid, 1);
        assert_eq!(etime, "04:12:33");
        assert_eq!(rest, "/usr/bin/claude --session-id abc  --foo");
    }

    #[test]
    fn split_ps_line_too_few_fields_is_none() {
        assert!(split_ps_line("1234 1").is_none());
        assert!(split_ps_line("").is_none());
    }

    #[test]
    fn is_claude_command_matches_basename_and_versioned_path() {
        assert!(is_claude_command("claude --foo"));
        assert!(is_claude_command("/usr/local/bin/claude"));
        assert!(is_claude_command(
            "/home/u/.local/share/claude/versions/2.1.0/claude"
        ));
        assert!(!is_claude_command("/usr/bin/claude-code-helper"));
        assert!(!is_claude_command("vim claude.txt"));
    }

    #[test]
    fn parse_flag_value_reads_space_and_equals_forms() {
        assert_eq!(
            parse_flag_value("claude --session-id abc-123 --model opus", "--session-id"),
            Some("abc-123".to_string())
        );
        assert_eq!(
            parse_flag_value("claude --model=opus", "--model"),
            Some("opus".to_string())
        );
        assert_eq!(parse_flag_value("claude --resume", "--session-id"), None);
    }

    #[test]
    fn fmt_uptime_covers_every_bucket() {
        assert_eq!(fmt_uptime("00:45"), "<1m");
        assert_eq!(fmt_uptime("22:10"), "22m");
        assert_eq!(fmt_uptime("04:12:33"), "4h12m");
        assert_eq!(fmt_uptime("11-03:00:00"), "11d3h");
    }

    #[test]
    fn infra_label_matches_every_known_infra_subcommand_and_flag() {
        assert_eq!(
            infra_label("/x/claude daemon run --origin transient"),
            Some("claude infrastructure (daemon)")
        );
        assert_eq!(
            infra_label("claude bg-pty-host --bg-pty-host /tmp/x.sock"),
            Some("claude infrastructure (bg-pty-host)")
        );
        assert_eq!(
            infra_label("claude bg-spare --bg-spare /tmp/x.claim.sock"),
            Some("claude infrastructure (bg-spare)")
        );
        assert_eq!(
            infra_label("/x/claude --chrome-native-host"),
            Some("claude infrastructure (chrome-native-host)")
        );
        assert_eq!(infra_label("claude --session-id abc"), None);
        assert_eq!(infra_label("claude"), None);
    }

    struct Scripted(Mutex<VecDeque<ProcRunOutput>>);

    #[async_trait::async_trait]
    impl ProcRunner for Scripted {
        async fn run(&self, _name: &str, _args: &[String]) -> ProcRunOutput {
            self.0.lock().unwrap().pop_front().unwrap_or(ProcRunOutput {
                stdout: String::new(),
                exit_code: 1,
                exec_error: None,
            })
        }
    }

    fn ok(stdout: &str) -> ProcRunOutput {
        ProcRunOutput {
            stdout: stdout.to_string(),
            exit_code: 0,
            exec_error: None,
        }
    }

    #[tokio::test]
    async fn scan_skips_classification_for_infra_subcommands() {
        let runner = Scripted(Mutex::new(VecDeque::from([
            ok("1001 1 01:00:00 /x/claude daemon run --origin transient\n\
                1002 1001 00:10:00 claude bg-pty-host --bg-pty-host /tmp/x.sock\n\
                2000 500 00:05:00 claude --session-id abc\n"),
            ok("500"),
            ok("1"),
            ProcRunOutput {
                stdout: String::new(),
                exit_code: 1,
                exec_error: None,
            },
            ok("pts/3"),
        ])));
        let procs = scan(&runner).await.unwrap();
        assert_eq!(procs.len(), 3);
        assert_eq!(procs[0].orphan, OrphanKind::NotOrphan);
        assert_eq!(procs[0].host, "claude infrastructure (daemon)");
        assert_eq!(procs[1].orphan, OrphanKind::NotOrphan);
        assert_eq!(procs[1].host, "claude infrastructure (bg-pty-host)");
        assert_eq!(procs[2].orphan, OrphanKind::NotOrphan);
        assert_eq!(procs[2].host, "terminal (pts/3)");
    }

    #[tokio::test]
    async fn live_scan_runs_against_the_real_ps_when_opted_in() {
        if std::env::var("MIKE_LIVE_PROCS").as_deref() != Ok("1") {
            return;
        }
        let runner = super::super::runner::SystemRunner;
        let procs = scan(&runner).await.expect("ps must succeed on a real host");
        let _ = procs.len();
    }
}
