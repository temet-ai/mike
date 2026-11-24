use uuid::Uuid;

use super::fixtures_tests::Db;

const CHECK: &str = "repeated_messages";

async fn store(db: &Db, request: i64, ord: i32, text: &str) -> anyhow::Result<()> {
    sqlx::query(
        "WITH stored AS (
             INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT md5($2), jsonb_build_object('role', 'user', 'content', $2), length($2), r.id
               FROM messages.roles r WHERE r.code = 'user'
             RETURNING id)
         INSERT INTO requests.messages (request_id, message_id, ord)
         SELECT $1, stored.id, $3 FROM stored",
    )
    .bind(request)
    .bind(text)
    .bind(ord)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

async fn link(db: &Db, request: i64, rule: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE requests.requests
            SET predecessor_rule_id = (SELECT id FROM requests.predecessor_rules WHERE code = $2)
          WHERE id = $1",
    )
    .bind(request)
    .bind(rule)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn repeated_messages_session_storing_its_messages_again_reports_the_rule()
-> anyhow::Result<()> {
    let db = Db::boot().await?;
    let session = db.session(Uuid::new_v4(), "1 minute").await?;
    let first = db.request(session, "10 minutes", "primary").await?;
    let resend = db.request(session, "5 minutes", "primary").await?;
    link(&db, resend, "lane_fallback").await?;
    for n in 0..60 {
        store(&db, first, n, &format!("turn {n}")).await?;
        store(&db, resend, n, &format!("turn {n}")).await?;
    }
    store(&db, resend, 60, "turn 60").await?;

    let findings = db.findings(CHECK).await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "repeated messages: lane_fallback");
    assert_eq!(findings[0].evidence["repeats"], 60);
    assert_eq!(findings[0].evidence["stored"], 61);
    db.cleanup().await
}

#[tokio::test]
async fn repeated_messages_copies_in_other_sessions_report_nothing() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    for _ in 0..2 {
        let session = db.session(Uuid::new_v4(), "1 minute").await?;
        let request = db.request(session, "5 minutes", "primary").await?;
        link(&db, request, "thread_start").await?;
        for n in 0..60 {
            store(&db, request, n, &format!("opening {n}")).await?;
        }
    }
    assert!(db.findings(CHECK).await?.is_empty());
    db.cleanup().await
}
