//! Request writes record which filter rules fired, in the junction table.

use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support as test_util;

#[tokio::test]
async fn insert_request_initial_writes_firing_rule_junction_rows() -> anyhow::Result<()> {
    use crate::filter::{FilterAction, FilterKind, FilterRule, RuleSource};

    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let rule_first = FilterRule {
        id: 0,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::TextSubstring,
        value: format!("junction-first-{}", uuid::Uuid::new_v4()),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };
    let rule_second = FilterRule {
        id: 0,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::XmlTag,
        value: format!("junction-second-{}", uuid::Uuid::new_v4()),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };
    let (fr_first_id, _) = handle.insert_filter_rule(&rule_first).await?;
    let (fr_second_id, _) = handle.insert_filter_rule(&rule_second).await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(
        Some(crate::store::models::RequestId::from_raw(95_001)),
        &session_id,
    );
    req.system_prompt = crate::store::models::SystemPromptObservation {
        prompt_sha256: Some(format!("{:064x}", 0x5151)),
        blocks: Some(serde_json::json!([])),
        composition_id: None,
        composition_applied: false,
        unknown_blocks: None,
        filter_modified: true,
        firing_rule_ids: vec![fr_first_id, fr_second_id],
        source: crate::store::models::SystemPromptSource::Exact,
    };

    let request_id = handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?
        .id;

    #[derive(sqlx::FromRow)]
    struct JunctionRow {
        ord: i32,
        rule_uuid: uuid::Uuid,
    }
    let rows = sqlx::query_as!(
        JunctionRow,
        r#"SELECT j.ord AS "ord!", fr.uuid AS "rule_uuid!"
                 FROM requests.system_prompt_firing_rules j
                 JOIN requests.system_prompts rsp
                   ON rsp.id = j.request_system_prompt_id
                 JOIN rules.rules fr ON fr.id = j.filter_rule_id
                WHERE rsp.request_id = $1
                ORDER BY j.ord ASC"#,
        request_id,
    )
    .fetch_all(&handle.pool)
    .await?;

    assert_eq!(
        rows.len(),
        2,
        "exactly two firing-rule junction rows must be written"
    );
    assert_eq!(rows[0].ord, 0, "first fired rule must have ord 0");
    assert_eq!(
        rows[0].rule_uuid, rule_first.uuid,
        "ord 0 must resolve to the first fired rule's uuid"
    );
    assert_eq!(rows[1].ord, 1, "second fired rule must have ord 1");
    assert_eq!(
        rows[1].rule_uuid, rule_second.uuid,
        "ord 1 must resolve to the second fired rule's uuid"
    );

    db.cleanup().await?;
    Ok(())
}
