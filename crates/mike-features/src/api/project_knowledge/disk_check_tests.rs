use serde_json::json;
use sha2::{Digest, Sha256};

use super::disk_check::{Indexed, OnDisk, annotate, inspect};

fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn indexed<'a>(root: &'a str, body: &'a str, content_hash: &'a str) -> Indexed<'a> {
    Indexed {
        root,
        relative_path: "CONFIGURATION.md",
        content_hash,
        body,
    }
}

#[tokio::test]
async fn inspect_reports_a_file_that_matches_the_indexed_hash() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("CONFIGURATION.md"), "same")?;
    let root = dir.path().to_string_lossy().into_owned();
    let content_hash = hash("same");
    assert_eq!(
        inspect(&indexed(&root, "same", &content_hash)).await,
        OnDisk::Matches
    );
    Ok(())
}

#[tokio::test]
async fn inspect_reports_both_lengths_when_the_file_grew_after_indexing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("CONFIGURATION.md"), "grown on disk")?;
    let root = dir.path().to_string_lossy().into_owned();
    let content_hash = hash("old");
    assert_eq!(
        inspect(&indexed(&root, "old", &content_hash)).await,
        OnDisk::Differs {
            indexed_characters: 3,
            disk_characters: 13
        }
    );
    Ok(())
}

#[tokio::test]
async fn inspect_distinguishes_a_deleted_file_from_a_root_on_another_machine() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_string_lossy().into_owned();
    let content_hash = hash("x");
    assert_eq!(
        inspect(&indexed(&root, "x", &content_hash)).await,
        OnDisk::Missing
    );
    let elsewhere = dir.path().join("absent").to_string_lossy().into_owned();
    assert!(matches!(
        inspect(&indexed(&elsewhere, "x", &content_hash)).await,
        OnDisk::NotChecked(_)
    ));
    Ok(())
}

#[tokio::test]
async fn annotate_marks_a_current_read_stale_when_the_file_differs() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("CONFIGURATION.md"), "grown on disk")?;
    let mut read = json!({
        "current": true, "deleted": false, "body": "old", "content_hash": hash("old"),
        "root_path": dir.path().to_string_lossy(), "path": "CONFIGURATION.md",
        "freshness": {"state": "fresh", "reasons": []}
    });
    annotate(&mut read).await;
    assert_eq!(read["freshness"]["state"], "stale");
    assert_eq!(read["on_disk"]["state"], "differs");
    let reasons = read["freshness"]["reasons"].to_string();
    assert!(reasons.contains("13 characters"), "{reasons}");
    assert!(reasons.contains("indexed copy 3"), "{reasons}");
    Ok(())
}

#[tokio::test]
async fn annotate_leaves_a_historical_revision_unchecked_and_fresh_state_alone()
-> anyhow::Result<()> {
    let mut read = json!({
        "current": false, "deleted": false, "body": "old", "content_hash": hash("old"),
        "root_path": "/nowhere", "path": "CONFIGURATION.md",
        "freshness": {"state": "fresh", "reasons": []}
    });
    annotate(&mut read).await;
    assert_eq!(read["freshness"]["state"], "fresh");
    assert_eq!(read["on_disk"]["state"], "not_checked");
    Ok(())
}

#[tokio::test]
async fn annotate_ignores_reads_that_are_not_artifacts() {
    let mut read = json!({"reference": "lesson:1", "body": "text"});
    annotate(&mut read).await;
    assert!(read.get("on_disk").is_none());
}
