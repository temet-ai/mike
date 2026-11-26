use std::path::Path;

use super::batch_land::BatchEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::worktree::{self, is_dated_batch_dir};
use super::{batch_states, tick};

pub(super) fn ignore_target(bench: &Bench) -> anyhow::Result<()> {
    let exclude = bench.ctx.repo.root.join(".git/info/exclude");
    std::fs::create_dir_all(exclude.parent().unwrap_or(&bench.ctx.repo.root))?;
    std::fs::write(exclude, "target/\n")?;
    Ok(())
}

#[test]
fn is_dated_batch_dir_dated_folder_true_shared_folder_false() {
    assert!(is_dated_batch_dir(Path::new(
        "/r/.claude/worktrees/wright-fixes-2026-11-24"
    )));
    assert!(!is_dated_batch_dir(Path::new("/r/.claude/worktrees/wright-fixes")));
    assert!(!is_dated_batch_dir(Path::new("/r/.claude/worktrees/wright-fixesx")));
}

#[tokio::test]
async fn next_day_batch_landed_yesterday_reuses_the_folder_and_its_build_output()
-> anyhow::Result<()> {
    let bench = Bench::boot(
        WritesFiles::new(&[])
            .then(&[("src/calc.sh", FIXED)])
            .then(&[("src/b.txt", "two\n")]),
    )
    .await?;
    bench.defect_in("src/calc.sh").await?;
    tick(&bench.ctx, true, true).await?;
    let yesterday = bench.only_batch().await?;
    let dir = bench.batch_dir();
    ignore_target(&bench)?;
    std::fs::create_dir_all(dir.join("target"))?;
    std::fs::write(dir.join("target/kept"), "build output")?;
    bench.backdate(yesterday.id).await?;
    bench.defect_in("src/b.txt").await?;

    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(report.batches, vec![(yesterday.id, BatchEnd::Landed)]);
    let batches = bench.batches().await?;
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[1].state, batch_states::OPEN);
    assert_eq!(batches[1].worktree, batches[0].worktree);
    assert_eq!(batches[1].worktree, dir.display().to_string());
    assert!(dir.join("target/kept").exists(), "the build output carries over");
    assert_eq!(
        sh(&dir, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        bench.batch_branch()
    );
    let listed = sh(&bench.ctx.repo.root, &["worktree", "list", "--porcelain"])?;
    assert_eq!(listed.matches("wright-fixes").count(), 1, "{listed}");
    bench.cleanup().await
}

#[tokio::test]
async fn today_waits_while_yesterday_in_the_shared_folder_is_still_landing()
-> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    let yesterday = bench.open_batch().await?;
    bench.backdate(yesterday.id).await?;
    sqlx::query("UPDATE wright.fix_batches SET state = 'gating', closed_at = now() WHERE id = $1")
        .bind(yesterday.id)
        .execute(bench.ctx.persist.pool())
        .await?;

    let opened = super::batch::open_today(&bench.ctx, chrono::Utc::now().date_naive()).await?;
    assert!(opened.is_none());
    assert_eq!(bench.batches().await?.len(), 1);
    bench.cleanup().await
}

#[tokio::test]
async fn dated_batch_folder_from_before_is_removed_when_its_batch_ends() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    let dated = bench.ctx.repo.worktrees().join("wright-fixes-2026-11-24");
    let branch = "wright/fixes-2026-11-24".to_owned();
    worktree::open_batch(&bench.ctx.runner, &bench.ctx.repo, &dated, &branch, &branch)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(dated.exists());

    worktree::remove_batch(&bench.ctx.runner, &bench.ctx.repo, &dated, &[branch.clone()])
        .await?;
    assert!(!dated.exists());
    assert!(sh(&bench.ctx.repo.root, &["branch", "--list", &branch])?.is_empty());
    bench.cleanup().await
}
