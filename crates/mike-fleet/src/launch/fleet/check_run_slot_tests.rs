use crate::launch::fleet::check::head;
use crate::launch::fleet::managed_test_support as managed;

use super::tests::{git, project};

const REWRITE: &str = "printf 'rewritten\\n' > mine/a.txt";
const SEE_ORIGINAL: &str = "test \"$(cat mine/a.txt)\" = ok";

#[tokio::test]
async fn a_check_that_rewrites_a_tracked_file_does_not_block_the_next_check() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let (_repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;

    def.check.command = REWRITE.to_string();
    let (first, first_output) = managed::check(&def, &at, "fleet-slot-rewrite-first").await?;
    assert!(
        first_output.contains("put back"),
        "{first:?}: {first_output}"
    );

    std::fs::write(at.slot.join("mine/a.txt"), "rewritten\n")?;

    def.check.command = SEE_ORIGINAL.to_string();
    let (second, second_output) = managed::check(&def, &at, "fleet-slot-rewrite-second").await?;
    assert!(
        !second_output.contains("has changes"),
        "the second check was refused: {second_output}"
    );
    assert_eq!(second, Some(0), "{second_output}");
    assert!(
        second_output.contains("left in its checkout"),
        "{second_output}"
    );

    let status = git(
        &at.slot,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .await?;
    assert_eq!(status, "", "the slot is clean afterwards");
    Ok(())
}
