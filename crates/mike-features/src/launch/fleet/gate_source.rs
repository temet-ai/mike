use super::gate::Target;
use super::gate_slot::isolated::git;

pub async fn verify(target: &Target) -> Result<(), String> {
    let head = git(&target.slot, &["rev-parse", "HEAD"]).await?;
    if head != target.commit {
        return Err(format!(
            "the isolated checkout is at {head}, not gate commit {}; no checkout was changed",
            target.commit
        ));
    }
    let changed = git(
        &target.slot,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .await?;
    if !changed.is_empty() {
        return Err(format!(
            "the isolated checkout has uncommitted changes; inspect them before retrying. No files were stashed or reset:\n{changed}"
        ));
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    async fn project() -> anyhow::Result<(tempfile::TempDir, Target)> {
        let root = tempfile::tempdir()?;
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "test"],
            vec!["commit", "--allow-empty", "-qm", "base"],
        ] {
            git(root.path(), &args).await.map_err(anyhow::Error::msg)?;
        }
        let commit = git(root.path(), &["rev-parse", "HEAD"])
            .await
            .map_err(anyhow::Error::msg)?;
        let target = Target {
            commit,
            pushed: false,
            slot: root.path().to_path_buf(),
        };
        Ok((root, target))
    }

    #[tokio::test]
    async fn clean_head_is_verified_and_untracked_work_is_preserved() -> anyhow::Result<()> {
        let (root, target) = project().await?;
        assert!(verify(&target).await.is_ok());
        std::fs::write(root.path().join("peer.txt"), "peer work")?;
        let problem = verify(&target).await.err().unwrap_or_default();
        assert!(problem.contains("uncommitted changes"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("peer.txt"))?,
            "peer work"
        );
        Ok(())
    }

    #[tokio::test]
    async fn wrong_head_is_refused_without_checkout_mutation() -> anyhow::Result<()> {
        let (root, mut target) = project().await?;
        let original = target.commit.clone();
        target.commit = "not-the-head".into();
        assert!(verify(&target).await.is_err());
        assert_eq!(
            git(root.path(), &["rev-parse", "HEAD"])
                .await
                .map_err(anyhow::Error::msg)?,
            original
        );
        Ok(())
    }

    #[tokio::test]
    async fn non_repository_cannot_claim_verified_source() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let target = Target {
            commit: "unknown".into(),
            pushed: false,
            slot: root.path().into(),
        };
        assert!(verify(&target).await.is_err());
        Ok(())
    }
}
