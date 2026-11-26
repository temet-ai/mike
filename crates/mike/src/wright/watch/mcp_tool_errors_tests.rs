use uuid::Uuid;

use super::fixtures_tests::Db;

#[tokio::test]
async fn mcp_tool_errors_skips_fleet_target_refusals() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let refusals = [
        r#"inspect API returned 400: {"error":"task T3 has no seat yet: nothing has been started for it"}"#,
        r#"inspect API returned 400: {"error":"no other seat of the fleet is open"}"#,
        r#"inspect API returned 400: {"error":"the fleet has no task `T9`; its tasks are T1, T2"}"#,
    ];
    for _ in 0..25 {
        let session = db.session(Uuid::new_v4(), "1 minute").await?;
        let request = db.request(session, "10 minutes", "primary").await?;
        for body in refusals {
            db.tool_error(request, session, "mcp__mike__ask", body).await?;
        }
    }
    for _ in 0..3 {
        let session = db.session(Uuid::new_v4(), "1 minute").await?;
        let request = db.request(session, "10 minutes", "primary").await?;
        db.tool_error(request, session, "mcp__mike__ask", "ask refused: no peer")
            .await?;
    }
    let findings = db.findings("mcp_tool_errors").await?;
    let resources: Vec<&str> = findings.iter().map(|f| f.resource.as_str()).collect();
    assert_eq!(resources, ["mcp__mike__ask: ask refused: no peer"]);
    db.cleanup().await
}
