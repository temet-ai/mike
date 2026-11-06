use mike::store::persistence::migrations::assess;

use crate::test_db::TestDb;

#[tokio::test]
async fn assess_fresh_migrated_db_is_clean() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;
    let assessment = assess(&pool).await?;
    pool.close().await;

    assert!(
        !assessment.is_blocking(),
        "fresh DB assessment must not be blocking; mismatched={:?}, missing={:?}, \
         out_of_order={:?}",
        assessment.mismatched,
        assessment.missing_from_binary,
        assessment.out_of_order
    );
    assert!(
        assessment.pending.is_empty(),
        "fresh DB assessment must have no pending migrations; pending={:?}",
        assessment.pending
    );

    handle_db.cleanup().await?;
    Ok(())
}
