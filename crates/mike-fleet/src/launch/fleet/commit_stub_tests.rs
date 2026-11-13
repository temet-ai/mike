use serde_json::json;
use uuid::Uuid;

use super::{NO_DB, scratch_def, scratch_repo, scratch_seated};
use crate::launch::fleet::commit::{CommitRequest, commit};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::watch::git;

async fn said(repo: &str, args: &[&str]) -> anyhow::Result<String> {
    let bytes = git(repo, args, None)
        .await
        .ok_or_else(|| anyhow::anyhow!("git {args:?} failed"))?;
    Ok(String::from_utf8(bytes)?.trim_end().to_owned())
}

fn shared_definition(repo: &str) -> anyhow::Result<FleetDefinition> {
    let mut value = serde_json::to_value(scratch_def(repo))?;
    value["shared_files"] = json!(["shared.rs"]);
    value["shared_file_producers"] = json!({"shared.rs": "M"});
    value["tasks"]["M"]["files"] = json!(["mine", "shared.rs"]);
    value["tasks"]["T"]["files"] = json!(["theirs", "shared.rs"]);
    FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)
}

#[tokio::test]
async fn shared_scaffold_commits_before_peer_work_and_accepts_a_later_named_contribution()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(586);
    let peer = Uuid::from_u128(5861);
    let def = shared_definition(&repo)?;
    let seated = scratch_seated(session).worker("T", peer, "db-dev-T");
    std::fs::write(dir.path().join("theirs/b.txt"), "peer staged work\n")?;
    said(&repo, &["add", "theirs/b.txt"]).await?;
    std::fs::write(dir.path().join("theirs/new.txt"), "peer unfinished\n")?;
    std::fs::write(dir.path().join("mine/a.txt"), "unrelated producer work\n")?;
    let peer_index = said(&repo, &["ls-files", "--stage", "--", "theirs/b.txt"]).await?;
    let mut previous = said(&repo, &["rev-parse", "HEAD"]).await?;
    for (author, body) in [
        (session, "pub struct Shared;\n"),
        (peer, "pub struct Shared { pub ready: bool }\n"),
    ] {
        std::fs::write(dir.path().join("shared.rs"), body)?;
        let report = commit(
            &def,
            &seated,
            &CommitRequest {
                session: author,
                message: "feat: shared scaffold contribution".into(),
                paths: vec!["shared.rs".into()],
                allow_empty: false,
            },
            NO_DB,
        )
        .await;
        assert_eq!(report.refused, None, "{report:?}");
        assert_eq!(report.paths, vec!["shared.rs"]);
        let sha = report
            .sha
            .ok_or_else(|| anyhow::anyhow!("missing commit"))?;
        assert_ne!(sha, previous);
        assert_eq!(said(&repo, &["rev-parse", "HEAD^"]).await?, previous);
        assert_eq!(
            said(&repo, &["show", "HEAD:shared.rs"]).await?,
            body.trim_end()
        );
        assert_eq!(
            said(
                &repo,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
            )
            .await?,
            "shared.rs"
        );
        assert_eq!(said(&repo, &["show", "HEAD:theirs/b.txt"]).await?, "one");
        assert_eq!(said(&repo, &["show", "HEAD:mine/a.txt"]).await?, "one");
        assert_eq!(
            said(&repo, &["ls-files", "--stage", "--", "theirs/b.txt"]).await?,
            peer_index
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("theirs/new.txt"))?,
            "peer unfinished\n"
        );
        previous = sha;
    }
    Ok(())
}
