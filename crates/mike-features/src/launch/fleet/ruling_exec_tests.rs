use super::*;
use crate::launch::fleet::ruling::{Ruling, definition};

#[tokio::test]
async fn ruling_runs_its_exact_contract_at_historical_sha_without_using_shared_edits()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let original = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    seat_commit(&repo, "mine/a.txt", "bad\n").await?;
    let changed = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::write(repo.join("mine/a.txt"), "peer's uncommitted work\n")?;
    let rule = Ruling {
        version: 1,
        command: "grep -q bad mine/a.txt".into(),
        expected_exit_code: 1,
    };
    let def = definition(&def, &rule);
    let (code, output) = managed::check(&def, &original, "fleet#ruling:one#baseline:old").await?;
    assert_eq!(code, Some(0), "{output}");
    let (code, output) = managed::check(&def, &changed, "fleet#ruling:one#baseline:new").await?;
    assert_eq!(code, Some(1), "{output}");
    assert_eq!(
        std::fs::read_to_string(repo.join("mine/a.txt"))?,
        "peer's uncommitted work\n"
    );
    assert_eq!(base_run("fleet#ruling:one#baseline:new"), "fleet");
    Ok(())
}

#[tokio::test]
async fn ruling_that_changes_the_clean_checkout_cannot_produce_a_passing_receipt()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, def) = project(root.path()).await?;
    let target = target(&def, Some("HEAD"))
        .await
        .map_err(anyhow::Error::msg)?;
    let rule = Ruling {
        version: 1,
        command: "printf modified > mine/a.txt; exit 0".into(),
        expected_exit_code: 0,
    };
    let def = definition(&def, &rule);
    let (code, output) = managed::check(&def, &target, "fleet#ruling:one#baseline:old").await?;
    assert_eq!(code, None);
    assert!(output.contains("Isolated result is not valid"), "{output}");
    Ok(())
}
