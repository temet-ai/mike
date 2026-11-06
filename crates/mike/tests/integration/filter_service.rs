use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::json;
use tokio::sync::Mutex;

use crate::test_db::{self, TestDb, TestDbHandle};

use mike::config::FilterWriteGuard;
use mike::filter::FilterConfig;
use mike::filter::service::{
    FilterRuleCreateInput, FilterRulePatchInput, FilterService, FilterServiceError,
};
use mike::store::persistence::PersistHandle;
use sqlx::PgPool;

const IDENTITY_LINE: &str = "You are Claude Code, Anthropic's official CLI for Claude.";
const MUTABLE_TAIL_TEXT: &str = "project guidance in the mutable tail";

fn claude_code_187_body() -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({
        "model": "claude-opus-5",
        "system": [
            {"type": "text", "text": "cch=aaa cc_prev_req=bbb"},
            {"type": "text", "text": IDENTITY_LINE},
            {
                "type": "text",
                "text": "main prompt body",
                "cache_control": {"type": "ephemeral", "scope": "global", "ttl": "1h"},
            },
            {
                "type": "text",
                "text": MUTABLE_TAIL_TEXT,
                "cache_control": {"type": "ephemeral", "ttl": "1h"},
            },
        ],
        "messages": [{"role": "user", "content": "hi"}],
    }))?)
}

async fn seeded_service(db: &TestDbHandle) -> anyhow::Result<Arc<FilterService>> {
    let handle = PersistHandle::connect(&db.database_url).await?;
    let seed_pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&seed_pool, "/tmp/issue-187").await?;
    test_db::seed_captured_request(
        &seed_pool,
        &uuid::Uuid::new_v4().to_string(),
        project_id,
        "primary_first",
        &claude_code_187_body()?,
    )
    .await?;

    let filter_config = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
    let write_lock = Arc::new(Mutex::new(FilterWriteGuard));
    FilterService::reconcile_on_startup(filter_config, write_lock, handle).await
}

fn create_input(value: &str, text: &str) -> FilterRuleCreateInput {
    FilterRuleCreateInput {
        kind: "text_substring".to_string(),
        value: value.to_string(),
        action: "append".to_string(),
        text: Some(text.to_string()),
        trigger_token: None,
        inject_text: None,
        enabled: true,
        project_path: None,
        style_override: false,
        title: None,
    }
}

#[tokio::test]
async fn create_reaches_the_live_arcswap_without_a_restart() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let filter_config = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
    let write_lock = Arc::new(Mutex::new(FilterWriteGuard));
    let svc =
        FilterService::reconcile_on_startup(filter_config.clone(), write_lock, handle.clone())
            .await?;
    assert!(
        filter_config.load_full().rules.is_empty(),
        "a fresh ephemeral DB seeds no rules"
    );

    let envelope = svc
        .create(FilterRuleCreateInput {
            kind: "text_substring".to_string(),
            value: "anchor phrase".to_string(),
            action: "append".to_string(),
            text: Some(" EXTRA".to_string()),
            trigger_token: None,
            inject_text: None,
            enabled: true,
            project_path: None,
            style_override: false,
            title: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;

    let current = filter_config.load_full();
    assert_eq!(
        current.rules.len(),
        1,
        "the new rule must be live in-memory"
    );
    assert_eq!(current.rules[0].id, envelope.rule.id);
    assert_eq!(current.rules[0].value, "anchor phrase");

    let filter_config2 = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
    let write_lock2 = Arc::new(Mutex::new(FilterWriteGuard));
    let _svc2 =
        FilterService::reconcile_on_startup(filter_config2.clone(), write_lock2, handle).await?;
    let after_restart = filter_config2.load_full();
    assert_eq!(after_restart.rules.len(), 1);
    assert_eq!(after_restart.rules[0].id, envelope.rule.id);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_removes_the_rule_and_it_never_reappears_on_reload() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let filter_config = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
    let write_lock = Arc::new(Mutex::new(FilterWriteGuard));
    let svc =
        FilterService::reconcile_on_startup(filter_config.clone(), write_lock, handle.clone())
            .await?;

    let envelope = svc
        .create(create_input("delete me", " EXTRA"))
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;
    assert_eq!(filter_config.load_full().rules.len(), 1);

    svc.delete(envelope.rule.id)
        .await
        .map_err(|e| anyhow::anyhow!("delete failed: {e}"))?;

    assert!(
        filter_config.load_full().rules.is_empty(),
        "a deleted rule must leave the live ArcSwap immediately"
    );

    let reloaded = handle.list_filter_rules().await?;
    assert!(
        reloaded.is_empty(),
        "a deleted rule must not reappear on a fresh DB read (round-trip of the actual bug: \
         the old delete only flipped enabled=false, so the rule kept coming back as a \
         disabled edit on the next refresh), got {reloaded:?}"
    );

    let second_delete = svc.delete(envelope.rule.id).await;
    assert!(
        matches!(second_delete, Err(FilterServiceError::NotFound)),
        "deleting an already-deleted rule must 404, not silently succeed again"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_warns_when_anchor_sits_in_the_protected_prefix() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let svc = seeded_service(&db).await?;

    let envelope = svc
        .create(create_input(
            IDENTITY_LINE,
            "\n\n# Communicating with the user\n...",
        ))
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;

    assert_eq!(
        envelope.warnings.len(),
        1,
        "identity-line anchor must carry exactly one warning, got {:?}",
        envelope.warnings
    );
    assert!(
        envelope.warnings[0].starts_with("anchor_unreachable"),
        "unexpected warning text: {}",
        envelope.warnings[0]
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_does_not_warn_when_anchor_is_reachable() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let svc = seeded_service(&db).await?;

    let envelope = svc
        .create(create_input(MUTABLE_TAIL_TEXT, " EXTRA"))
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;

    assert!(
        envelope.warnings.is_empty(),
        "reachable anchor must not warn, got {:?}",
        envelope.warnings
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn update_warns_when_an_edit_moves_the_anchor_into_the_protected_prefix() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let svc = seeded_service(&db).await?;

    let created = svc
        .create(create_input(MUTABLE_TAIL_TEXT, " EXTRA"))
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;
    assert!(
        created.warnings.is_empty(),
        "the mutable-tail anchor must not warn at create time"
    );

    let updated = svc
        .update(
            created.rule.id,
            FilterRulePatchInput {
                value: Some(IDENTITY_LINE.to_string()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("update failed: {e}"))?;

    assert_eq!(
        updated.warnings.len(),
        1,
        "re-anchoring onto the identity line must warn on the update response, got {:?}",
        updated.warnings
    );
    assert!(
        updated.warnings[0].starts_with("anchor_unreachable"),
        "unexpected warning text: {}",
        updated.warnings[0]
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_does_not_warn_when_no_request_has_ever_been_captured() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let filter_config = Arc::new(ArcSwap::from_pointee(FilterConfig::default()));
    let write_lock = Arc::new(Mutex::new(FilterWriteGuard));
    let svc = FilterService::reconcile_on_startup(filter_config, write_lock, handle).await?;

    let envelope = svc
        .create(create_input(
            IDENTITY_LINE,
            "\n\n# Communicating with the user\n...",
        ))
        .await
        .map_err(|e| anyhow::anyhow!("create failed: {e}"))?;

    assert!(
        envelope.warnings.is_empty(),
        "with no captured traffic to check against there is nothing to warn about, got {:?}",
        envelope.warnings
    );

    db.cleanup().await?;
    Ok(())
}
