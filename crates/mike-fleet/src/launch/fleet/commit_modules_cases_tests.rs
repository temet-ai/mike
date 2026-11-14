use uuid::Uuid;

use super::{NO_DB, scratch_def, scratch_repo, scratch_seated};
use crate::launch::fleet::commit::{CommitRequest, commit};

fn ask(session: Uuid) -> CommitRequest {
    CommitRequest {
        session,
        message: "feat: list".into(),
        paths: Vec::new(),
        allow_empty: false,
    }
}

#[tokio::test]
async fn a_commit_declaring_a_module_whose_file_is_not_committed_lands_with_a_warning()
-> anyhow::Result<()> {
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(31);
    let mut def = scratch_def(&repo);
    if let Some(theirs) = def.tasks.get_mut("T") {
        theirs.new_files.push("mine/peer.rs".to_string());
    }
    std::fs::write(dir.path().join("mine/mod.rs"), "mod there;\nmod peer;\n")?;
    std::fs::write(dir.path().join("mine/there.rs"), "pub fn f() {}\n")?;
    std::fs::write(dir.path().join("mine/peer.rs"), "pub fn g() {}\n")?;
    let report = commit(&def, &scratch_seated(session), &ask(session), NO_DB).await;
    assert_eq!(report.refused, None, "{report:?}");
    assert!(
        report.warnings.is_empty(),
        "peer.rs is in the commit, so nothing is missing: {:?}",
        report.warnings
    );
    std::fs::write(
        dir.path().join("mine/mod.rs"),
        "mod there;\nmod peer;\nmod gone;\n",
    )?;
    let report = commit(&def, &scratch_seated(session), &ask(session), NO_DB).await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("mine/mod.rs declares `mod gone;`")
            && report.warnings[0].contains("mine/gone.rs"),
        "{}",
        report.warnings[0]
    );
    Ok(())
}

#[tokio::test]
async fn a_module_file_a_peer_owns_and_has_not_committed_names_the_peers_task() -> anyhow::Result<()>
{
    let (dir, repo) = scratch_repo().await?;
    let session = Uuid::from_u128(32);
    let mut def = scratch_def(&repo);
    if let Some(theirs) = def.tasks.get_mut("T") {
        theirs.new_files.push("mine/peer.rs".to_string());
    }
    std::fs::write(dir.path().join("mine/mod.rs"), "mod peer;\n")?;
    let report = commit(&def, &scratch_seated(session), &ask(session), NO_DB).await;
    assert_eq!(report.refused, None, "{report:?}");
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("Task T (theirs) lists it"),
        "{}",
        report.warnings[0]
    );
    Ok(())
}
