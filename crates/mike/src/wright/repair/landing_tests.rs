use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;

use super::attempt::AttemptEnd;
use super::batch_land::BatchEnd;
use super::fixtures_tests::{Bench, FIXED, PrState, WritesFiles, sh};
use super::runner::{Cmd, Program};
use super::{batch_states, states, tick};

pub(super) fn three_fixes() -> WritesFiles {
    WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then(&[("src/b.txt", "two\n")])
        .then(&[("src/c.txt", "three\n")])
}

pub(super) async fn three_batched(bench: &Bench) -> anyhow::Result<[i64; 3]> {
    let mut findings = [0; 3];
    for (slot, part) in findings
        .iter_mut()
        .zip(["src/calc.sh", "src/b.txt", "src/c.txt"])
    {
        let (_, finding) = bench.defect_in(part).await?;
        *slot = finding;
        let report = tick(&bench.ctx, true).await?;
        assert_eq!(
            report.attempted.map(|(_, end)| end),
            Some(AttemptEnd::Batched),
            "{report:?}"
        );
    }
    Ok(findings)
}

const ALLOWED: [&str; 24] = [
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
    "git reset",
    "git clean",
    "git log",
    "git show",
    "git rebase",
    "git revert",
    "git merge",
    "gh pr create",
    "gh pr merge",
    "gh pr view",
    "gh pr close",
];

fn verb(cmd: &Cmd) -> String {
    let mut args = cmd.args.iter().map(String::as_str);
    let mut words = Vec::new();
    let wanted = if cmd.program == Program::Gh { 2 } else { 1 };
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
async fn three_fixes_of_a_day_land_through_one_gate_and_one_pull_request() -> anyhow::Result<()> {
    let bench = Bench::boot(three_fixes()).await?;
    assert!(bench.scratch.defect_replays()?);
    let before = bench.scratch.remote_main()?;
    let findings = three_batched(&bench).await?;
    let batch = bench.only_batch().await?;
    assert_eq!(batch.state, batch_states::OPEN);
    assert_eq!(
        bench.scratch.gate_log(),
        "",
        "nothing is gated during the day"
    );
    assert_eq!(bench.scratch.remote_main()?, before);
    assert!(bench.batch_dir().exists());

    bench.backdate(batch.id).await?;
    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(batch.id, BatchEnd::Landed)]);

    let landed = bench
        .ctx
        .persist
        .web()
        .wright_fix_batch(batch.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch vanished"))?;
    assert_eq!(landed.state, batch_states::LANDED, "{:?}", landed.reason);
    let merge = landed.merge_sha.clone().unwrap_or_default();
    assert_eq!(merge, bench.scratch.remote_main()?);
    let log = bench.scratch.gate_log();
    assert_eq!(log.lines().count(), 1, "one push hook run: {log}");
    assert!(log.starts_with("pass "));
    let gated = log.trim_start_matches("pass ").trim().to_owned();
    assert_eq!(
        tree(&bench.scratch.remote, &merge)?,
        tree(&bench.scratch.remote, &gated)?
    );
    let prs = bench.ctx.runner.prs();
    assert_eq!(prs.len(), 1);
    assert!(matches!(prs[0].state, PrState::Merged(_)));
    let programs = bench.ctx.runner.programs();
    let create = programs
        .iter()
        .find(|line| line.starts_with("gh pr create"))
        .cloned()
        .unwrap_or_default();
    assert!(
        create.contains(&format!("--title Wright fixes for {}", landed.day)),
        "{create}"
    );
    for finding in findings {
        let repair = bench.repair_of(finding).await?;
        assert_eq!(repair.state, states::LANDED, "{:?}", repair.reason);
        assert_eq!(repair.merge_sha.as_deref(), Some(merge.as_str()));
        assert!(
            create.contains(&format!("repair {}", repair.id)),
            "{create}"
        );
    }
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    assert_eq!(bench.scratch.on_main("src/b.txt")?, "two");
    assert_eq!(bench.scratch.on_main("src/c.txt")?, "three");
    assert!(
        sh(
            &bench.scratch.root,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}", landed.branch)
            ]
        )
        .is_err(),
        "the landed batch's branch is deleted"
    );
    for cmd in bench
        .ctx
        .runner
        .commands
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default()
    {
        let verb = verb(&cmd);
        if cmd.program != Program::Cx {
            assert!(
                ALLOWED.contains(&verb.as_str()),
                "unexpected command {}",
                cmd.line()
            );
        }
        if cmd.line().contains(" push ") {
            assert!(
                !cmd.line().contains("--force") && !cmd.line().contains(" +"),
                "force push: {}",
                cmd.line()
            );
        }
    }
    bench.cleanup().await
}

#[tokio::test]
async fn batch_regates_on_a_new_branch_when_main_moves_after_the_gate() -> anyhow::Result<()> {
    let bench = Bench::boot(three_fixes()).await?;
    three_batched(&bench).await?;
    let batch = bench.only_batch().await?;
    bench.backdate(batch.id).await?;
    if let Ok(mut moves) = bench.ctx.runner.move_main_on_create.lock() {
        *moves = 1;
    }

    tick(&bench.ctx, true).await?;
    let landed = bench
        .ctx
        .persist
        .web()
        .wright_fix_batch(batch.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch vanished"))?;
    assert_eq!(landed.state, batch_states::LANDED, "{:?}", landed.reason);
    let first = landed.branches.first().cloned().unwrap_or_default();
    assert_eq!(landed.branches, vec![first.clone(), format!("{first}-2")]);
    let log = bench.scratch.gate_log();
    let gated: Vec<&str> = log.lines().collect();
    assert_eq!(gated.len(), 2, "both branches pass the push hook: {log}");
    assert_ne!(gated[0], gated[1]);
    let second = gated[1].trim_start_matches("pass ");
    assert_eq!(
        bench
            .scratch
            .remote_branch(&format!("{first}-2"))
            .as_deref(),
        Some(second)
    );
    let prs = bench.ctx.runner.prs();
    assert_eq!(prs.len(), 2);
    assert_eq!(prs[0].state, PrState::Closed);
    assert!(matches!(prs[1].state, PrState::Merged(_)));
    assert_eq!(bench.scratch.on_main("docs/peer.md")?, "a peer landed this");
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    for repair in bench.repairs_in(batch.id).await? {
        assert_eq!(repair.state, states::LANDED);
        assert_eq!(repair.merge_sha, landed.merge_sha);
    }
    bench.cleanup().await
}

#[tokio::test]
async fn batch_leaves_other_sessions_worktrees_untouched() -> anyhow::Result<()> {
    let bench = Bench::boot(three_fixes()).await?;
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
    three_batched(&bench).await?;
    let batch = bench.only_batch().await?;
    bench.backdate(batch.id).await?;
    tick(&bench.ctx, true).await?;

    assert_eq!(
        std::fs::read_to_string(peer.join("src/calc.sh"))?,
        "a peer's unfinished edit\n"
    );
    assert!(
        sh(&root, &["worktree", "list", "--porcelain"])?.contains(&format!("worktree {shown}"))
    );
    assert_eq!(sh(&peer, &["status", "--porcelain"])?, "M src/calc.sh");
    bench.cleanup().await
}

#[tokio::test]
async fn repair_does_not_start_in_a_path_that_is_not_the_days_worktree() -> anyhow::Result<()> {
    let bench = Bench::boot(three_fixes()).await?;
    let taken = bench.batch_dir();
    std::fs::create_dir_all(&taken)?;
    std::fs::write(taken.join("notes.txt"), "mine\n")?;
    let (_, finding) = bench.defect().await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.attempted, None);
    let repairs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wright.repairs WHERE finding_id = $1")
            .bind(finding)
            .fetch_one(bench.ctx.persist.pool())
            .await?;
    assert_eq!(repairs, 0);
    assert_eq!(std::fs::read_to_string(taken.join("notes.txt"))?, "mine\n");
    assert!(bench.ctx.engineer.jobs().is_empty());
    assert_eq!(bench.scratch.gate_log(), "");
    bench.cleanup().await
}
