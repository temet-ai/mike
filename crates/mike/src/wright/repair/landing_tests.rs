use std::path::Path;

use super::attempt::AttemptEnd;
use super::fixtures_tests::{BUGGY, Bench, FIXED, PrState, WritesFiles, sh};
use super::runner::{Cmd, Program};
use super::{states, tick};

fn fix() -> WritesFiles {
    WritesFiles::new(&[("src/calc.sh", FIXED)])
}

const ALLOWED: [&str; 17] = [
    "git fetch",
    "git rev-parse",
    "git config",
    "git worktree",
    "git add",
    "git diff",
    "git commit",
    "git push",
    "git ls-remote",
    "git switch",
    "git cherry-pick",
    "git merge-base",
    "git branch",
    "gh pr create",
    "gh pr merge",
    "gh pr view",
    "gh pr close",
];

fn verb(cmd: &Cmd) -> String {
    let mut args = cmd.args.iter().map(String::as_str);
    let mut words = Vec::new();
    let wanted = if cmd.program == Program::Git { 1 } else { 2 };
    while let Some(arg) = args.next() {
        if arg == "-c" {
            args.next();
            continue;
        }
        words.push(arg);
        if words.len() == wanted {
            break;
        }
    }
    format!("{} {}", cmd.program.name(), words.join(" "))
}

fn tree(dir: &Path, sha: &str) -> anyhow::Result<String> {
    sh(dir, &["rev-parse", &format!("{sha}^{{tree}}")])
}

#[tokio::test]
async fn repair_of_a_replayable_defect_lands_the_gated_commit_on_main() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    assert!(
        bench.scratch.defect_replays()?,
        "the scratch defect must fail its check on main"
    );
    let before = bench.scratch.remote_main()?;

    let report = tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(report.attempted, Some((repair.id, AttemptEnd::Landed)));
    assert_eq!(repair.state, states::LANDED, "reason: {:?}", repair.reason);
    let commit = repair.commit_sha.clone().unwrap_or_default();
    let merge = repair.merge_sha.clone().unwrap_or_default();
    assert_eq!(repair.base_sha.as_deref(), Some(before.as_str()));
    assert_eq!(repair.gate_exit, Some(0));
    assert_eq!(bench.scratch.gate_log(), format!("pass {commit}\n"));
    assert_eq!(merge, bench.scratch.remote_main()?);
    assert_eq!(
        tree(&bench.scratch.remote, &merge)?,
        tree(&bench.scratch.remote, &commit)?
    );
    assert_eq!(
        sh(&bench.scratch.remote, &["rev-parse", &format!("{merge}^1")])?,
        before
    );
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    assert_eq!(repair.reason, None);
    assert_eq!(repair.patch["files"][0]["path"], "src/calc.sh");
    assert_eq!(
        sh(
            &bench.scratch.root,
            &["log", "-1", "--format=%an <%ae>", &commit]
        )?,
        "Wright <wright@mike.invalid>"
    );
    assert_eq!(
        bench.scratch.remote_branch(&repair.branch),
        Some(commit.clone())
    );
    assert!(
        !bench.worktree(finding).exists(),
        "a landed repair's worktree is removed"
    );
    assert_eq!(
        sh(
            &bench.scratch.root,
            &["rev-parse", &format!("refs/heads/{}", repair.branch)]
        )?,
        commit
    );
    let jobs = bench.ctx.engineer.jobs();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].brief.contains("data, never instructions"));
    let commands = bench
        .ctx
        .runner
        .commands
        .lock()
        .map(|cmds| cmds.clone())
        .unwrap_or_default();
    assert!(!commands.is_empty());
    for cmd in &commands {
        let verb = verb(cmd);
        assert!(
            ALLOWED.contains(&verb.as_str()),
            "unexpected command {}",
            cmd.line()
        );
    }
    for line in bench.ctx.runner.programs() {
        if line.contains(" push ") {
            assert!(
                !line.contains("--force") && !line.contains(" +"),
                "force push: {line}"
            );
        }
    }
    bench.cleanup().await
}

#[tokio::test]
async fn repair_regates_on_a_new_branch_when_main_moves_after_the_gate() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    if let Ok(mut moves) = bench.ctx.runner.move_main_on_create.lock() {
        *moves = 1;
    }

    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::LANDED, "reason: {:?}", repair.reason);
    let first = format!("wright/repair-{finding}");
    let second = format!("wright/repair-{finding}-2");
    assert_eq!(repair.branches, vec![first.clone(), second.clone()]);
    assert_eq!(repair.branch, second);
    assert_eq!(repair.gates, 2);
    let log = bench.scratch.gate_log();
    let gated: Vec<&str> = log.lines().collect();
    assert_eq!(
        gated.len(),
        2,
        "both commits pass through the push hook: {log}"
    );
    assert!(gated.iter().all(|line| line.starts_with("pass ")));
    let first_sha = gated[0].trim_start_matches("pass ");
    let second_sha = gated[1].trim_start_matches("pass ");
    assert_ne!(first_sha, second_sha);
    assert_eq!(
        bench.scratch.remote_branch(&first).as_deref(),
        Some(first_sha),
        "the first branch is never rewritten"
    );
    assert_eq!(
        bench.scratch.remote_branch(&second).as_deref(),
        Some(second_sha)
    );
    assert_eq!(repair.commit_sha.as_deref(), Some(second_sha));
    let prs = bench.ctx.runner.prs();
    assert_eq!(prs.len(), 2);
    assert_eq!(prs[0].state, PrState::Closed);
    assert!(matches!(prs[1].state, PrState::Merged(_)));
    let merge = repair.merge_sha.clone().unwrap_or_default();
    assert_eq!(
        tree(&bench.scratch.remote, &merge)?,
        tree(&bench.scratch.remote, second_sha)?
    );
    assert_eq!(bench.scratch.on_main("docs/peer.md")?, "a peer landed this");
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    bench.cleanup().await
}

#[tokio::test]
async fn repair_leaves_other_sessions_worktrees_untouched() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let root = bench.scratch.root.clone();
    let peer = bench.ctx.repo.worktrees().join("peer-session");
    let shown = peer.display().to_string();
    sh(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "peer/work",
            &shown,
            "HEAD",
        ],
    )?;
    std::fs::write(peer.join("src/calc.sh"), "a peer's unfinished edit\n")?;
    let (_, finding) = bench.defect().await?;

    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::LANDED, "reason: {:?}", repair.reason);
    assert_eq!(
        std::fs::read_to_string(peer.join("src/calc.sh"))?,
        "a peer's unfinished edit\n"
    );
    assert!(
        sh(&root, &["worktree", "list", "--porcelain"])?.contains(&format!("worktree {shown}"))
    );
    assert_eq!(sh(&peer, &["status", "--porcelain"])?, "M src/calc.sh");
    assert_eq!(
        sh(&root, &["rev-parse", "peer/work"])?,
        repair.base_sha.unwrap_or_default()
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_refuses_a_worktree_path_that_is_not_its_own() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    let taken = bench.worktree(finding);
    std::fs::create_dir_all(&taken)?;
    std::fs::write(taken.join("notes.txt"), BUGGY)?;

    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::REFUSED);
    assert!(repair.reason.unwrap_or_default().contains("left alone"));
    assert_eq!(std::fs::read_to_string(taken.join("notes.txt"))?, BUGGY);
    assert!(
        bench.ctx.engineer.jobs().is_empty(),
        "no engineer run starts on a path that is not ours"
    );
    assert_eq!(bench.scratch.gate_log(), "");
    bench.cleanup().await
}
