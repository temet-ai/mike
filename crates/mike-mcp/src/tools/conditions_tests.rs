use serde_json::json;

use super::*;

#[test]
fn wait_for_arguments_serialise_only_what_was_given() -> anyhow::Result<()> {
    let args: WaitForArgs = serde_json::from_value(json!({
        "kind": "time",
        "brief": "come back",
        "after_secs": 60,
        "select": ["/condition/state"],
    }))?;
    let body = serde_json::to_value(&args)?;
    assert_eq!(
        body,
        json!({"kind": "time", "brief": "come back", "after_secs": 60})
    );
    Ok(())
}

#[test]
fn wait_for_requires_kind_and_brief() {
    let missing = serde_json::from_value::<WaitForArgs>(json!({"kind": "time"}));
    assert!(missing.is_err());
}

#[test]
fn list_query_sends_only_the_given_filters() {
    let query = list_query(ListConditionsArgs {
        thread: Some("side-calls".to_owned()),
        state: None,
        session: None,
        limit: Some(5),
    });
    assert_eq!(
        query,
        vec![
            ("thread", "side-calls".to_owned()),
            ("limit", "5".to_owned())
        ]
    );
    assert!(list_query(ListConditionsArgs::default()).is_empty());
}

#[test]
fn wait_for_description_names_every_kind_and_action() {
    for word in [
        "`time`",
        "`session_turn_ended`",
        "`session_ended`",
        "`fleet_event`",
        "`sql`",
        "`words`",
        "`return`",
        "`start`",
        "`message`",
    ] {
        assert!(WAIT_FOR_DESCRIPTION.contains(word), "{word}");
    }
}
