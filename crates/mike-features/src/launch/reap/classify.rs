use std::fmt;

use super::ancestors::ancestors;
use super::runner::ProcRunner;
use crate::launch::TMUX_SESSION_PREFIX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrphanKind {
    NotOrphan,
    OrphanTmux(String),
    OrphanNoTty,
}

impl fmt::Display for OrphanKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrphanKind::NotOrphan => write!(f, "not orphan"),
            OrphanKind::OrphanTmux(session) => write!(f, "ORPHAN (tmux {session})"),
            OrphanKind::OrphanNoTty => write!(f, "ORPHAN (no tty)"),
        }
    }
}

struct TmuxPaneInfo {
    pane_pid: u32,
    session: String,
    attached: bool,
}

fn parse_tmux_pane_line(line: &str) -> Option<TmuxPaneInfo> {
    let mut fields = line.split('|');
    let pane_pid = fields.next()?.parse().ok()?;
    let session = fields.next()?.to_string();
    let attached = fields.next()? == "1";
    Some(TmuxPaneInfo {
        pane_pid,
        session,
        attached,
    })
}

async fn scan_tmux_panes(runner: &dyn ProcRunner) -> Vec<TmuxPaneInfo> {
    let args = [
        "list-panes".to_string(),
        "-a".to_string(),
        "-F".to_string(),
        "#{pane_pid}|#{session_name}|#{session_attached}".to_string(),
    ];
    let output = runner.run("tmux", &args).await;
    if !output.ok() {
        return Vec::new();
    }
    output
        .stdout
        .lines()
        .filter_map(parse_tmux_pane_line)
        .filter(|p| p.session.starts_with(TMUX_SESSION_PREFIX))
        .collect()
}

async fn pane_tty(runner: &dyn ProcRunner, pid: u32) -> Option<String> {
    let args = [
        "-o".to_string(),
        "tty=".to_string(),
        "-p".to_string(),
        pid.to_string(),
    ];
    let output = runner.run("ps", &args).await;
    output.ok().then(|| output.stdout.trim().to_string())
}

pub async fn classify_host(runner: &dyn ProcRunner, pid: u32) -> (String, OrphanKind) {
    let anc = ancestors(runner, pid).await;
    let panes = scan_tmux_panes(runner).await;
    for candidate in &anc {
        let Some(pane) = panes.iter().find(|p| p.pane_pid == *candidate) else {
            continue;
        };
        return if pane.attached {
            (format!("tmux {}", pane.session), OrphanKind::NotOrphan)
        } else {
            let kind = OrphanKind::OrphanTmux(pane.session.clone());
            (kind.to_string(), kind)
        };
    }
    match pane_tty(runner, pid).await {
        Some(tty) if !tty.is_empty() && tty != "?" && tty != "??" => {
            (format!("terminal ({tty})"), OrphanKind::NotOrphan)
        }
        _ => (OrphanKind::OrphanNoTty.to_string(), OrphanKind::OrphanNoTty),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::launch::reap::runner::ProcRunOutput;

    struct Scripted(Mutex<VecDeque<ProcRunOutput>>);

    fn ok(stdout: &str) -> ProcRunOutput {
        ProcRunOutput {
            stdout: stdout.to_string(),
            exit_code: 0,
            exec_error: None,
        }
    }

    fn failed() -> ProcRunOutput {
        ProcRunOutput {
            stdout: String::new(),
            exit_code: 1,
            exec_error: None,
        }
    }

    #[async_trait]
    impl ProcRunner for Scripted {
        async fn run(&self, _name: &str, _args: &[String]) -> ProcRunOutput {
            self.0.lock().unwrap().pop_front().unwrap_or_else(failed)
        }
    }

    #[tokio::test]
    async fn classify_attached_tmux_ancestor_is_not_orphan() {
        let runner = Scripted(Mutex::new(VecDeque::from([
            ok("500"),
            ok("1"),
            ok("500|mike-deadbeef|1"),
        ])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "tmux mike-deadbeef");
        assert_eq!(kind, OrphanKind::NotOrphan);
    }

    #[tokio::test]
    async fn classify_detached_tmux_ancestor_is_orphan_tmux() {
        let runner = Scripted(Mutex::new(VecDeque::from([
            ok("500"),
            ok("1"),
            ok("500|mike-abandon1|0"),
        ])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "ORPHAN (tmux mike-abandon1)");
        assert_eq!(kind, OrphanKind::OrphanTmux("mike-abandon1".into()));
    }

    #[tokio::test]
    async fn classify_detached_non_mike_tmux_ancestor_falls_through_to_tty_check() {
        let runner = Scripted(Mutex::new(VecDeque::from([
            ok("500"),
            ok("1"),
            ok("500|term-9999|0"),
            ok("pts/7"),
        ])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "terminal (pts/7)");
        assert_eq!(kind, OrphanKind::NotOrphan);
    }

    #[tokio::test]
    async fn classify_detached_non_mike_tmux_ancestor_with_no_tty_is_orphan_no_tty() {
        let runner = Scripted(Mutex::new(VecDeque::from([
            ok("500"),
            ok("1"),
            ok("500|term-9999|0"),
            ok("?"),
        ])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "ORPHAN (no tty)");
        assert_eq!(kind, OrphanKind::OrphanNoTty);
    }

    #[tokio::test]
    async fn classify_no_tmux_ancestor_real_tty_is_not_orphan() {
        let runner = Scripted(Mutex::new(VecDeque::from([ok("1"), failed(), ok("pts/4")])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "terminal (pts/4)");
        assert_eq!(kind, OrphanKind::NotOrphan);
    }

    #[tokio::test]
    async fn classify_no_tmux_ancestor_no_tty_is_orphan_no_tty() {
        let runner = Scripted(Mutex::new(VecDeque::from([ok("1"), failed(), ok("?")])));
        let (host, kind) = classify_host(&runner, 1000).await;
        assert_eq!(host, "ORPHAN (no tty)");
        assert_eq!(kind, OrphanKind::OrphanNoTty);
    }

    #[test]
    fn parse_tmux_pane_line_reads_three_fields() {
        let pane = parse_tmux_pane_line("1234|mysession|1").unwrap();
        assert_eq!(pane.pane_pid, 1234);
        assert_eq!(pane.session, "mysession");
        assert!(pane.attached);
        assert!(parse_tmux_pane_line("garbage").is_none());
    }

    #[test]
    fn orphan_kind_display_matches_the_contract_labels() {
        assert_eq!(OrphanKind::OrphanNoTty.to_string(), "ORPHAN (no tty)");
        assert_eq!(
            OrphanKind::OrphanTmux("s1".into()).to_string(),
            "ORPHAN (tmux s1)"
        );
    }
}
