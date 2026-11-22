use super::{JSON_NAME, Live, LiveFleet, TEXT_NAME, write_if_changed};

fn live(summary: &str) -> Live {
    Live {
        fleets: vec![LiveFleet {
            slug: "drill".to_owned(),
            fleet_uuid: "u".to_owned(),
            project_path: "/p".to_owned(),
            summary: summary.to_owned(),
        }],
    }
}

#[tokio::test]
async fn write_if_changed_writes_both_files_once_per_change() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut last = None;
    write_if_changed(dir.path(), &live("drill 1 of 3 passed"), &mut last).await;
    assert_eq!(
        std::fs::read_to_string(dir.path().join(TEXT_NAME))?,
        "drill 1 of 3 passed\n"
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(JSON_NAME))?)?;
    assert_eq!(json["fleets"][0]["project_path"], "/p");

    std::fs::remove_file(dir.path().join(TEXT_NAME))?;
    write_if_changed(dir.path(), &live("drill 1 of 3 passed"), &mut last).await;
    assert!(!dir.path().join(TEXT_NAME).exists(), "an unchanged summary is not rewritten");

    write_if_changed(dir.path(), &Live { fleets: Vec::new() }, &mut last).await;
    assert_eq!(std::fs::read_to_string(dir.path().join(TEXT_NAME))?, "");
    Ok(())
}
