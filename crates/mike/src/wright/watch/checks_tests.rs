use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::bounded;
use super::fixtures_tests::Db;

#[tokio::test]
async fn request_failures_opens_one_finding_per_error_shape_seen_five_times() -> anyhow::Result<()>
{
    let db = Db::boot().await?;
    let session = db.session(Uuid::new_v4(), "1 minute").await?;
    for n in 0..5 {
        let request = db.request(session, "5 minutes", "primary").await?;
        db.request_error(request, &format!("stream aborted after {n} chunks"))
            .await?;
    }
    let lone = db.request(session, "5 minutes", "primary").await?;
    db.request_error(lone, "upstream 529 overloaded").await?;
    let old = db.request(session, "3 hours", "primary").await?;
    db.request_error(old, "stream aborted after 9 chunks")
        .await?;

    let findings = db.findings("request_failures").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].scope, "capture");
    assert_eq!(findings[0].resource, "stream aborted after N chunks");
    assert_eq!(findings[0].evidence["failed_requests"], 5);
    db.cleanup().await
}

#[tokio::test]
async fn mcp_tool_errors_groups_by_tool_and_error_kind() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let session = db.session(Uuid::new_v4(), "1 minute").await?;
    let request = db.request(session, "10 minutes", "primary").await?;
    for n in 0..20 {
        db.tool_error(
            request,
            session,
            "mcp__mike__find_text",
            &format!("needle {n} is empty"),
        )
        .await?;
    }
    for _ in 0..19 {
        db.tool_error(
            request,
            session,
            "mcp__mike__get_request",
            "request not found",
        )
        .await?;
    }
    for _ in 0..25 {
        db.tool_error(request, session, "Bash", "Exit code 1")
            .await?;
    }
    let findings = db.findings("mcp_tool_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].resource,
        "mcp__mike__find_text: needle N is empty"
    );
    assert_eq!(findings[0].evidence["failures"], 20);
    db.cleanup().await
}

#[tokio::test]
async fn hook_errors_reads_the_hook_and_its_reason() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let session = db.session(Uuid::new_v4(), "1 minute").await?;
    let request = db.request(session, "10 minutes", "primary").await?;
    for _ in 0..10 {
        db.tool_error(
            request,
            session,
            "Bash",
            "PreToolUse:Bash hook error: Refused: `sed -i` on ten files or fewer.",
        )
        .await?;
    }
    let findings = db.findings("hook_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].scope, "hook");
    assert_eq!(findings[0].evidence["hook"], "PreToolUse:Bash");
    assert_eq!(
        findings[0].resource,
        "PreToolUse:Bash: Refused: `sed -i` on ten files or fewer."
    );
    assert_eq!(db.findings("session_tool_errors").await?, Vec::new());
    db.cleanup().await
}

#[tokio::test]
async fn session_tool_errors_needs_three_sessions_hitting_the_same_error() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let mut sessions = Vec::new();
    for _ in 0..3 {
        let session = db.session(Uuid::new_v4(), "1 minute").await?;
        sessions.push((session, db.request(session, "10 minutes", "primary").await?));
    }
    for (session, request) in &sessions {
        for _ in 0..4 {
            db.tool_error(*request, *session, "Read", "File does not exist: /tmp/x")
                .await?;
        }
    }
    for _ in 0..12 {
        db.tool_error(sessions[0].1, sessions[0].0, "Grep", "regex parse error")
            .await?;
    }
    let findings = db.findings("session_tool_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "Read: File does not exist: /tmp/x");
    assert_eq!(findings[0].evidence["sessions"], 3);
    db.cleanup().await
}

async fn llm_calls(db: &Db, role: &str, outcome: &str, n: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO llm.calls (role, model, outcome)
         SELECT $1, 'test-model', $2 FROM generate_series(1, $3)",
    )
    .bind(role)
    .bind(outcome)
    .bind(n)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn llm_role_errors_flags_a_role_failing_half_its_calls() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    llm_calls(&db, "session_judge", "timeout", 6).await?;
    llm_calls(&db, "session_judge", "ok", 4).await?;
    llm_calls(&db, "steerer", "transport_error", 2).await?;
    llm_calls(&db, "steerer", "ok", 10).await?;
    llm_calls(&db, "eval", "refused_spend", 12).await?;
    let findings = db.findings("llm_role_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "llm_role:session_judge");
    assert_eq!(findings[0].evidence["failed"], 6);
    db.cleanup().await
}

#[tokio::test]
async fn scheduled_job_failures_reads_the_latest_run_per_job_and_machine() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    sqlx::query(
        "INSERT INTO jobs.runs (job_name, machine_uuid, trigger, started_at, finished_at, duration_ms,
                                result_kind, exit_code, output_tail, consecutive_failures)
         VALUES ('cargo_sweep', NULL, 'schedule', now() - interval '2 hours', now() - interval '2 hours', 5,
                 'exited', 1, 'disk full', 2),
                ('usage_audit', NULL, 'schedule', now() - interval '3 hours', now() - interval '3 hours', 5,
                 'exited', 1, 'boom', 3),
                ('usage_audit', NULL, 'schedule', now() - interval '1 hour', now() - interval '1 hour', 5,
                 'succeeded', 0, 'ok', 0)",
    )
    .execute(db.handle.pool())
    .await?;
    let findings = db.findings("scheduled_job_failures").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "job:cargo_sweep");
    db.cleanup().await
}

#[tokio::test]
async fn page_client_errors_reads_what_the_page_reported() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    for n in 0..3 {
        db.handle
            .web()
            .insert_wright_client_error(&crate::store::persistence::wright::NewClientError {
                route: "/threads".to_owned(),
                kind: "load".to_owned(),
                message: format!("Failed to fetch dynamically imported module {n}"),
                detail: json!({}),
            })
            .await?;
    }
    let findings = db.findings("page_client_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].resource,
        "/threads load: Failed to fetch dynamically imported module N"
    );
    assert_eq!(findings[0].evidence["reports"], 3);
    db.cleanup().await
}

#[tokio::test]
async fn a_query_that_fails_reports_its_error_instead_of_no_findings() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let result = bounded::findings(
        db.handle.pool(),
        "broken",
        "SELECT * FROM wright.no_such_table",
        &json!({}),
    )
    .await;
    let Err(reason) = result else {
        anyhow::bail!("a failed query must not read as a clean pass");
    };
    assert!(reason.contains("broken: query failed"), "{reason}");
    let slow = bounded::findings(
        db.handle.pool(),
        "slow",
        "SELECT 'x'::text, 'y'::text, '{}'::jsonb FROM pg_sleep(11)",
        &json!({}),
    )
    .await;
    let Err(reason) = slow else {
        anyhow::bail!("a query past its deadline must not read as a clean pass");
    };
    assert!(
        reason.contains("statement timeout") || reason.contains("no answer"),
        "{reason}"
    );
    db.cleanup().await
}

#[tokio::test]
async fn checks_read_through_a_read_only_transaction() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let result: Result<Vec<(String, String, Value)>, String> = bounded::rows(
        db.handle.pool(),
        "writer",
        "INSERT INTO wright.client_errors (route, kind, message) VALUES ('/', 'load', 'x')
         RETURNING route, kind, detail",
        &json!({}),
    )
    .await;
    let Err(reason) = result else {
        anyhow::bail!("a check must not be able to write");
    };
    assert!(reason.contains("read-only"), "{reason}");
    db.cleanup().await
}

#[tokio::test]
async fn the_seed_holds_every_check_with_its_thresholds_in_params() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let checks = db.handle.web().list_wright_checks().await?;
    let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "cold_forks",
            "decisions_health",
            "hook_errors",
            "llm_role_errors",
            "log_errors",
            "log_warnings",
            "mcp_tool_errors",
            "page_client_errors",
            "request_failures",
            "scheduled_job_failures",
            "session_tool_errors",
            "slow_queries",
            "undelivered_mail",
        ]
    );
    assert!(checks.iter().all(|c| c.enabled));
    let warnings = checks.iter().find(|c| c.name == "log_warnings");
    assert_eq!(
        warnings.map(|c| c.params.clone()),
        Some(json!({"level": "WARN", "repeats": 100, "window_minutes": 60}))
    );
    db.cleanup().await
}

#[tokio::test]
async fn mcp_tool_errors_fires_when_one_error_hits_many_sessions_once_each() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    for _ in 0..14 {
        let session = db.session(Uuid::new_v4(), "1 minute").await?;
        let request = db.request(session, "10 minutes", "primary").await?;
        db.tool_error(request, session, "mcp__mike__ask", "ask refused: no peer")
            .await?;
    }
    let findings = db.findings("mcp_tool_errors").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "mcp__mike__ask: ask refused: no peer");
    assert_eq!(findings[0].evidence["failures"], 14);
    assert_eq!(findings[0].evidence["sessions"], 14);
    db.cleanup().await
}

#[tokio::test]
async fn changed_params_change_what_a_stored_check_reports() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let session = db.session(Uuid::new_v4(), "1 minute").await?;
    let request = db.request(session, "10 minutes", "primary").await?;
    for _ in 0..2 {
        db.tool_error(request, session, "mcp__mike__ask", "ask refused")
            .await?;
    }
    assert_eq!(db.findings("mcp_tool_errors").await?, Vec::new());
    let Some(row) = db.handle.web().wright_check("mcp_tool_errors").await? else {
        anyhow::bail!("the seed must hold mcp_tool_errors");
    };
    let mut changed = crate::store::persistence::wright::NewCheck::from(row);
    changed.params = json!({"window_hours": 24, "min_failures": 2, "min_sessions": 3});
    db.handle.web().replace_wright_check(&changed).await?;
    assert_eq!(db.findings("mcp_tool_errors").await?.len(), 1);
    db.cleanup().await
}

#[tokio::test]
async fn validate_refuses_a_failing_query_and_the_wrong_columns() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let pool = db.handle.pool();
    let params = json!({"n": 1});
    let ok = "SELECT 'a'::text, 'b'::text, jsonb_build_object('n', $1->>'n')";
    assert_eq!(bounded::validate(pool, ok, &params).await, Ok(()));

    let missing = bounded::validate(pool, "SELECT * FROM wright.no_such_table", &params).await;
    assert!(missing.is_err_and(|e| e.contains("no_such_table")));

    let two = bounded::validate(pool, "SELECT 'a'::text, 'b'::text", &params).await;
    assert!(two.is_err_and(|e| e.contains("(text, text, jsonb)")));

    let wrong = bounded::validate(pool, "SELECT 1, 'b'::text, '{}'::jsonb", &params).await;
    assert!(wrong.is_err_and(|e| e.contains("int4, text, jsonb")));

    let write = bounded::validate(
        pool,
        "WITH w AS (INSERT INTO wright.client_errors (route, kind, message)
                    VALUES ('/', 'load', 'x') RETURNING route)
         SELECT route, route, '{}'::jsonb FROM w",
        &params,
    )
    .await;
    assert!(write.is_err_and(|e| e.contains("read-only")));
    db.cleanup().await
}
