use chrono::{DateTime, Utc};
use mike::filter::{FilterAction, FilterKind, FilterRule, RuleSource};
use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

#[tokio::test]
async fn insert_filter_rule_does_not_inherit_updated_at_as_created_at() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let rule_id = uuid::Uuid::new_v4();
    let stale_updated_at = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2020, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("failed to construct stale timestamp"))?;

    let rule = FilterRule {
        id: 0,
        uuid: rule_id,
        kind: FilterKind::TextSubstring,
        value: format!("w8-regression-{}", uuid::Uuid::new_v4()),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: stale_updated_at,
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };

    handle.insert_filter_rule(&rule).await?;

    #[derive(sqlx::FromRow)]
    struct Timestamps {
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    }
    let ts = sqlx::query_as!(
        Timestamps,
        "SELECT created_at, updated_at FROM rules.rules WHERE uuid = $1",
        rule_id,
    )
    .fetch_one(handle.pool())
    .await?;

    let (created_at, updated_at) = (ts.created_at, ts.updated_at);
    assert_eq!(
        updated_at, stale_updated_at,
        "updated_at must round-trip the caller-supplied value"
    );
    let now = Utc::now();
    assert!(
        created_at > now - chrono::Duration::seconds(10),
        "created_at ({created_at}) must be close to NOW() ({now}), \
         not inherited from the stale updated_at ({stale_updated_at})"
    );

    Ok(())
}

#[tokio::test]
async fn insert_filter_rule_does_not_seed_composition_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let overrides_before: i64 =
        sqlx::query_scalar!("SELECT COUNT(*)::BIGINT FROM overrides.overrides")
            .fetch_one(handle.pool())
            .await?
            .unwrap_or(0);

    let rule_id = uuid::Uuid::new_v4();
    let rule = FilterRule {
        id: 0,
        uuid: rule_id,
        kind: FilterKind::XmlTag,
        value: format!("no-seed-{}", uuid::Uuid::new_v4()),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };
    handle.insert_filter_rule(&rule).await?;

    let overrides_after: i64 =
        sqlx::query_scalar!("SELECT COUNT(*)::BIGINT FROM overrides.overrides")
            .fetch_one(handle.pool())
            .await?
            .unwrap_or(0);
    assert_eq!(
        overrides_after, overrides_before,
        "insert_filter_rule must not create an overrides.overrides row"
    );

    let former_seed_sha: String = sqlx::query_scalar!(
        "SELECT encode(digest('filter:' || $1, 'sha256'), 'hex')",
        rule_id.to_string(),
    )
    .fetch_one(handle.pool())
    .await?
    .ok_or_else(|| anyhow::anyhow!("digest returned NULL"))?;
    let stub_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::BIGINT FROM overrides.overrides WHERE prompt_sha256 = $1",
        former_seed_sha,
    )
    .fetch_one(handle.pool())
    .await?
    .unwrap_or(0);
    assert_eq!(
        stub_count, 0,
        "no overrides.overrides row should exist at the former filter-stub sha"
    );

    handle_db.cleanup().await?;
    Ok(())
}
