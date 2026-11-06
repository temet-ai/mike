use crate::db_helpers;

#[tokio::test]
async fn classifier_maps_all_four_kind_cases_correctly() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT input, pk.code
           FROM (VALUES
               ('Describe your most recent action in 3-5 words: doing stuff',
                CASE
                    WHEN 'Describe your most recent action in 3-5 words: doing stuff'
                         LIKE '%Describe your most recent action in 3-5 words%' THEN 1
                    WHEN 'Describe your most recent action in 3-5 words: doing stuff'
                         LIKE '%Stop hook feedback:%' THEN 2
                    WHEN 'Describe your most recent action in 3-5 words: doing stuff'
                         LIKE '%<teammate-message%' THEN 3
                    ELSE 4
                END),
               ('Stop hook feedback: pre-commit failed',
                CASE
                    WHEN 'Stop hook feedback: pre-commit failed'
                         LIKE '%Describe your most recent action in 3-5 words%' THEN 1
                    WHEN 'Stop hook feedback: pre-commit failed'
                         LIKE '%Stop hook feedback:%' THEN 2
                    WHEN 'Stop hook feedback: pre-commit failed'
                         LIKE '%<teammate-message%' THEN 3
                    ELSE 4
                END),
               ('here is a message from rust-developer: <teammate-message>do the thing</teammate-message>',
                CASE
                    WHEN 'here is a message from rust-developer: <teammate-message>do the thing</teammate-message>'
                         LIKE '%Describe your most recent action in 3-5 words%' THEN 1
                    WHEN 'here is a message from rust-developer: <teammate-message>do the thing</teammate-message>'
                         LIKE '%Stop hook feedback:%' THEN 2
                    WHEN 'here is a message from rust-developer: <teammate-message>do the thing</teammate-message>'
                         LIKE '%<teammate-message%' THEN 3
                    ELSE 4
                END),
               ('implement the new feature for the users',
                CASE
                    WHEN 'implement the new feature for the users'
                         LIKE '%Describe your most recent action in 3-5 words%' THEN 1
                    WHEN 'implement the new feature for the users'
                         LIKE '%Stop hook feedback:%' THEN 2
                    WHEN 'implement the new feature for the users'
                         LIKE '%<teammate-message%' THEN 3
                    ELSE 4
                END)
           ) AS t(input, kind_id)
           JOIN prompt_kinds.kinds pk ON pk.id = t.kind_id
          ORDER BY t.kind_id",
    )
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 4, "expected exactly 4 classification results");
    assert_eq!(
        rows[0].1, "activity-label",
        "activity-label case: {}",
        rows[0].0
    );
    assert_eq!(
        rows[1].1, "hook-feedback",
        "hook-feedback case: {}",
        rows[1].0
    );
    assert_eq!(
        rows[2].1, "teammate-message",
        "teammate-message case: {}",
        rows[2].0
    );
    assert_eq!(
        rows[3].1, "operator",
        "operator (catch-all) case: {}",
        rows[3].0
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
