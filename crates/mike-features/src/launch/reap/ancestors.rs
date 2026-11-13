use super::MAX_ANCESTOR_HOPS;
use super::runner::ProcRunner;
use super::scan::ClaudeProcess;

pub async fn ancestors(runner: &dyn ProcRunner, pid: u32) -> Vec<u32> {
    let mut chain = vec![pid];
    let mut current = pid;
    for _ in 0..MAX_ANCESTOR_HOPS {
        let output = runner
            .run(
                "ps",
                &[
                    "-o".to_string(),
                    "ppid=".to_string(),
                    "-p".to_string(),
                    current.to_string(),
                ],
            )
            .await;
        if !output.ok() {
            break;
        }
        let Some(ppid) = output.stdout.trim().parse::<u32>().ok() else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        chain.push(ppid);
        current = ppid;
    }
    chain
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReapPlan {
    pub to_kill: Vec<ClaudeProcess>,
    pub skipped_self: Vec<ClaudeProcess>,
}

pub fn exclude_self(
    procs: Vec<ClaudeProcess>,
    self_pid: u32,
    ancestors: &[u32],
) -> (Vec<ClaudeProcess>, Vec<ClaudeProcess>) {
    let mut kept = Vec::with_capacity(procs.len());
    let mut skipped = Vec::new();
    for proc in procs {
        if proc.pid == self_pid || ancestors.contains(&proc.pid) {
            skipped.push(proc);
        } else {
            kept.push(proc);
        }
    }
    (kept, skipped)
}

pub fn reap_plan(procs: Vec<ClaudeProcess>, self_pid: u32, ancestors: &[u32]) -> ReapPlan {
    let eligible: Vec<ClaudeProcess> = procs
        .into_iter()
        .filter(|p| {
            matches!(
                p.orphan,
                super::classify::OrphanKind::OrphanTmux(_)
                    | super::classify::OrphanKind::OrphanNoTty
            )
        })
        .collect();
    let (to_kill, skipped_self) = exclude_self(eligible, self_pid, ancestors);
    ReapPlan {
        to_kill,
        skipped_self,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;
    use crate::launch::reap::classify::OrphanKind;
    use crate::launch::reap::runner::{ProcRunOutput, ProcRunner};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct ScriptedPpids(Mutex<VecDeque<Option<u32>>>);

    #[async_trait]
    impl ProcRunner for ScriptedPpids {
        async fn run(&self, _name: &str, _args: &[String]) -> ProcRunOutput {
            let mut queue = self.0.lock().unwrap();
            match queue.pop_front() {
                Some(Some(ppid)) => ProcRunOutput {
                    stdout: ppid.to_string(),
                    exit_code: 0,
                    exec_error: None,
                },
                Some(None) => ProcRunOutput {
                    stdout: String::new(),
                    exit_code: 1,
                    exec_error: None,
                },
                None => ProcRunOutput {
                    stdout: String::new(),
                    exit_code: 1,
                    exec_error: None,
                },
            }
        }
    }

    #[tokio::test]
    async fn ancestors_walks_up_to_pid_1_and_stops() {
        let runner = ScriptedPpids(Mutex::new(VecDeque::from([Some(500), Some(1)])));
        let chain = ancestors(&runner, 1000).await;
        assert_eq!(chain, vec![1000, 500]);
    }

    #[tokio::test]
    async fn ancestors_caps_at_max_hops() {
        let answers: VecDeque<Option<u32>> =
            (2..2 + MAX_ANCESTOR_HOPS as u32 + 5).map(Some).collect();
        let runner = ScriptedPpids(Mutex::new(answers));
        let chain = ancestors(&runner, 100_000).await;
        assert_eq!(chain.len(), MAX_ANCESTOR_HOPS + 1);
    }

    #[tokio::test]
    async fn ancestors_fails_open_on_a_probe_error() {
        let runner = ScriptedPpids(Mutex::new(VecDeque::from([None])));
        let chain = ancestors(&runner, 42).await;
        assert_eq!(chain, vec![42]);
    }

    fn proc(pid: u32, orphan: OrphanKind) -> ClaudeProcess {
        ClaudeProcess {
            pid,
            ppid: 1,
            uptime: "1m".into(),
            cwd: String::new(),
            session: None,
            model: None,
            host: orphan.to_string(),
            orphan,
        }
    }

    #[test]
    fn exclude_self_removes_self_and_ancestors() {
        let procs = vec![
            proc(10, OrphanKind::OrphanNoTty),
            proc(20, OrphanKind::OrphanNoTty),
            proc(30, OrphanKind::OrphanNoTty),
        ];
        let (kept, skipped) = exclude_self(procs, 10, &[20]);
        assert_eq!(kept.iter().map(|p| p.pid).collect::<Vec<_>>(), vec![30]);
        assert_eq!(
            skipped.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn reap_plan_never_includes_a_non_orphan() {
        let procs = vec![
            proc(1, OrphanKind::NotOrphan),
            proc(2, OrphanKind::OrphanTmux("s1".into())),
            proc(3, OrphanKind::OrphanNoTty),
        ];
        let plan = reap_plan(procs, 999, &[]);
        assert_eq!(
            plan.to_kill.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(plan.skipped_self.is_empty());
    }

    #[test]
    fn reap_plan_skips_the_caller_even_if_orphaned() {
        let procs = vec![proc(500, OrphanKind::OrphanNoTty)];
        let plan = reap_plan(procs, 500, &[500]);
        assert!(plan.to_kill.is_empty());
        assert_eq!(plan.skipped_self.len(), 1);
    }
}
