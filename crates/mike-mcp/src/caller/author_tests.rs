use super::*;
use serde_json::json;

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

fn args(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

async fn supplied(arguments: Value, session: Option<&str>) -> anyhow::Result<Map<String, Value>> {
    let client = InspectClient::new("http://127.0.0.1:1".into());
    supply_defaults(&client, "define_fleet", args(arguments), session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no arguments"))
}

#[tokio::test]
async fn define_fleet_creating_records_the_calling_session_as_its_author() -> anyhow::Result<()> {
    let prepared = supplied(json!({"slug": "a", "project_path": "/r"}), Some(SESSION)).await?;
    assert_eq!(prepared.get("created_by"), Some(&json!(SESSION)));
    Ok(())
}

#[tokio::test]
async fn define_fleet_changing_a_fleet_or_naming_an_author_keeps_the_arguments()
-> anyhow::Result<()> {
    let changing = supplied(json!({"fleet": "night-run", "patch": {}}), Some(SESSION)).await?;
    assert!(!changing.contains_key("created_by"));
    let named = supplied(
        json!({"slug": "a", "project_path": "/r", "created_by": "other"}),
        Some(SESSION),
    )
    .await?;
    assert_eq!(named.get("created_by"), Some(&json!("other")));
    Ok(())
}

#[tokio::test]
async fn define_fleet_without_a_valid_caller_session_sends_no_author() -> anyhow::Result<()> {
    for session in [None, Some("arbitrary")] {
        let prepared = supplied(json!({"slug": "a", "project_path": "/r"}), session).await?;
        assert!(!prepared.contains_key("created_by"));
    }
    Ok(())
}
