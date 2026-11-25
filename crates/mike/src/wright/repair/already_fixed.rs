use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::attempt::{AttemptEnd, Fix, abandon, rewind};
use super::follow::contains;
use super::runner::{CommandRunner, git};
use super::worktree::{MAIN_TRACKING, fetch_main};
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::RepairUpdate;

const SHOWN_COMMITS: &str = "--max-count=40";
pub(crate) const IN_FIRST_ROUND: &str = "listed in the first round";
pub(crate) const BRIEF: &str = r#"First check whether the defect is still there: main may have moved since the finding was written. /wright/in/finding.json lists the latest commits on main that touched the owning part under `main_changes`, and earlier attempts at this repair under `earlier_attempts`. When the code already behaves as the finding says it should, edit nothing and add to repair.json the field "already_fixed": { "commit": the commit on main that fixed it, "why": what in the code shows it }. The executor checks that the commit is on main, and Wright's own check judges afterwards whether the problem stays away."#;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AlreadyFixed {
    pub(crate) commit: String,
    pub(crate) why: String,
}

impl AlreadyFixed {
    pub(crate) fn from_output(output: &Value) -> Option<Self> {
        let named = output.get("already_fixed").filter(|v| !v.is_null())?;
        Some(
            serde_json::from_value(named.clone()).unwrap_or_else(|_| Self {
                commit: named.to_string(),
                why: String::new(),
            }),
        )
    }
}

fn is_sha(text: &str) -> bool {
    (7..=40).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) async fn main_changes<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    part: &str,
) -> String {
    let mut args = vec!["log", SHOWN_COMMITS, "--format=%h %cs %s", MAIN_TRACKING];
    if worktree.join(part).exists() {
        args.extend(["--", part]);
    }
    git(runner, worktree, &args).await.unwrap_or_default()
}

async fn on_main<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    fix: &Fix<'_>,
    commit: &str,
) -> Result<String, String>
where
    R: CommandRunner,
{
    let commit = commit.trim();
    if !is_sha(commit) {
        return Err(format!("`{commit}` is not a commit id"));
    }
    let main = fetch_main(&ctx.runner, &ctx.repo.root)
        .await
        .map_err(|e| format!("origin/main could not be read: {e}"))?;
    let object = format!("{commit}^{{commit}}");
    let full = git(
        &ctx.runner,
        fix.path,
        &["rev-parse", "--verify", "--quiet", &object],
    )
    .await
    .map_err(|_| format!("no commit {commit} exists"))?;
    if contains(&ctx.runner, &ctx.repo.root, &full, &main).await {
        Ok(full)
    } else {
        Err(format!("commit {full} is not on main {main}"))
    }
}

pub(super) async fn settle<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    fix: &Fix<'_>,
    fixed: &AlreadyFixed,
) -> Result<AttemptEnd, PersistError>
where
    R: CommandRunner,
{
    let full = match on_main(ctx, fix, &fixed.commit).await {
        Ok(full) => full,
        Err(why) => {
            let reason = format!(
                "the engineer run says the defect is already fixed, but {why}: {}",
                fixed.why
            );
            return abandon(ctx, fix, states::REFUSED, reason).await;
        }
    };
    rewind(ctx, fix).await;
    ctx.persist
        .web()
        .advance_wright_repair(&RepairUpdate {
            id: fix.claim.id,
            to: states::LANDED,
            from: &[states::EDITING],
            reason: Some(format!("already fixed on main by {full}: {}", fixed.why)),
            merge_sha: Some(full),
            ..RepairUpdate::default()
        })
        .await?;
    Ok(AttemptEnd::Ended)
}
