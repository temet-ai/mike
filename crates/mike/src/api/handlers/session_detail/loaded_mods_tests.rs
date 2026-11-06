use super::*;
use chrono::Utc;
use serde_json::json;

fn note(kind: &str, data: serde_json::Value) -> SessionNoteRow {
    SessionNoteRow {
        note_uuid: uuid::Uuid::new_v4(),
        kind: kind.to_string(),
        title: "t".to_string(),
        body: String::new(),
        status: None,
        tags: Vec::new(),
        data,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn latest_mods_listing_newest_mods_note_wins() -> anyhow::Result<()> {
    let rows = vec![
        note("mods", json!({"entries": [{"name": "newer"}]})),
        note("mods", json!({"entries": [{"name": "older"}]})),
    ];
    let listing = latest_mods_listing(rows).ok_or_else(|| anyhow::anyhow!("no listing"))?;
    assert_eq!(listing["entries"][0]["name"], "newer");
    Ok(())
}

#[test]
fn latest_mods_listing_ignores_other_kinds() {
    let rows = vec![note("finding", json!({"entries": []}))];
    assert!(latest_mods_listing(rows).is_none());
}

#[test]
fn latest_mods_listing_skips_a_note_whose_data_is_not_an_object() -> anyhow::Result<()> {
    let rows = vec![
        note("mods", json!("garbled")),
        note("mods", json!({"entries": []})),
    ];
    let listing = latest_mods_listing(rows).ok_or_else(|| anyhow::anyhow!("no listing"))?;
    assert!(listing["entries"].is_array());
    Ok(())
}

#[test]
fn latest_mods_listing_without_notes_is_none() {
    assert!(latest_mods_listing(Vec::new()).is_none());
}
