use std::time::Instant;

use mike::store::persistence::session_flow::FlowComponent;

use super::support::{Fixture, ids, no_raw};

#[tokio::test]
async fn isolated_thousand_request_projection_is_bounded_and_measured() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    mike::store::test_support::seed_session_turns(
        f.handle.pool(),
        &f.session.to_string(),
        f.project,
        1000,
    )
    .await?;
    let began = Instant::now();
    let page = f.page(FlowComponent::Requests, 100, None).await?;
    assert_eq!(ids(&page).len(), 100);
    assert!(!page.coverage.exhausted && page.next_cursor.is_some());
    println!(
        "P04 isolated request page: fixture_rows=1000 driving_rows=100 elapsed_ms={:.3} native_debug_bytes={}; SQL_count=4 plus transaction setup; wire JSON bytes belong to P12 conversion",
        began.elapsed().as_secs_f64() * 1000.0,
        format!("{page:?}").len()
    );
    let plan: serde_json::Value = sqlx::query_scalar("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
        SELECT id FROM requests.requests WHERE session_id = $1 AND id > 0 AND id <= $2 ORDER BY id LIMIT 101")
        .bind(f.id).bind(i64::MAX).fetch_one(f.handle.pool()).await?;
    println!("P04 isolated page driver EXPLAIN: {plan}");
    no_raw(&page);
    f.cleanup().await
}
