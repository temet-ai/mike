use chrono::{Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::fixtures_tests::Db;
use super::forks::{ColdCause, ColdFork, classify, findings_from};

fn fork(template_args: Value, fork_args: Value, idle_minutes: Option<i64>) -> ColdFork {
    let first_request_at = Utc::now();
    ColdFork {
        template: "rust".to_owned(),
        fork_session: Uuid::new_v4(),
        forked_at: first_request_at,
        fork_model: Some("opus".to_owned()),
        template_model: Some("opus".to_owned()),
        fork_args,
        template_args,
        first_request_id: 1,
        first_request_at,
        prompt_tokens: 90_000,
        template_last_request_at: idle_minutes.map(|m| first_request_at - Duration::minutes(m)),
    }
}

fn template_args() -> Value {
    json!(["--model", "opus", "--agent", "rust-developer"])
}

fn forked_args() -> Value {
    json!([
        "--resume",
        "abc",
        "--fork-session",
        "--model",
        "opus",
        "--agent",
        "rust-developer"
    ])
}

#[test]
fn classify_a_fork_whose_prefix_changed_within_the_cache_window_blames_the_prefix() {
    let changed = json!([
        "--resume",
        "abc",
        "--fork-session",
        "--model",
        "opus",
        "--agent",
        "other"
    ]);
    assert_eq!(
        classify(&fork(template_args(), changed, Some(10))),
        ColdCause::PrefixChanged
    );
}

#[test]
fn classify_a_fork_inside_the_cache_window_with_the_same_prefix_never_blames_expiry() {
    assert_eq!(
        classify(&fork(template_args(), forked_args(), Some(10))),
        ColdCause::Unexplained
    );
}

#[test]
fn classify_a_fork_after_the_cache_window_with_the_same_prefix_is_expiry() {
    assert_eq!(
        classify(&fork(template_args(), forked_args(), Some(90))),
        ColdCause::CacheExpired
    );
}

#[test]
fn classify_a_fork_of_a_template_that_never_sent_a_request_says_so() {
    assert_eq!(
        classify(&fork(template_args(), forked_args(), None)),
        ColdCause::TemplateNeverCached
    );
}

#[test]
fn classify_a_model_change_wins_over_every_other_cause() {
    let mut changed = fork(template_args(), json!(["--resume", "abc"]), Some(90));
    changed.fork_model = Some("sonnet".to_owned());
    assert_eq!(classify(&changed), ColdCause::ModelChanged);
}

#[test]
fn findings_from_groups_cold_forks_by_template_and_cause() {
    let forks = [
        fork(template_args(), forked_args(), Some(90)),
        fork(template_args(), forked_args(), Some(95)),
        fork(template_args(), forked_args(), Some(5)),
    ];
    let findings = findings_from(&forks);
    let resources: Vec<&str> = findings.iter().map(|f| f.resource.as_str()).collect();
    assert_eq!(resources, ["rust: cache_expired", "rust: unexplained"]);
    assert_eq!(findings[0].evidence["cold_forks"], 2);
}

async fn template(db: &Db, uuid: Uuid, args: &Value) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO launch.templates (name, session_uuid, folder, backend, model, claude_args)
         VALUES ('rust', $1, '/wright/project', 'none', 'opus', $2) RETURNING id",
    )
    .bind(uuid)
    .bind(args)
    .fetch_one(db.handle.pool())
    .await?)
}

async fn launch_fork(db: &Db, template: i64, uuid: Uuid, args: &Value) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.launches (session_uuid, backend, folder, template_id, model, claude_args, created_at)
         VALUES ($1, 'none', '/wright/project', $2, 'opus', $3, now() - interval '20 minutes')",
    )
    .bind(uuid)
    .bind(template)
    .bind(args)
    .execute(db.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn cold_forks_names_the_changed_prefix_not_the_cache_on_captured_data() -> anyhow::Result<()>
{
    let db = Db::boot().await?;
    let template_uuid = Uuid::new_v4();
    let template_session = db.session(template_uuid, "1 minute").await?;
    let warm = db
        .request(template_session, "25 minutes", "primary")
        .await?;
    db.response(warm, 0, 80_000).await?;
    let template_id = template(&db, template_uuid, &template_args()).await?;

    let changed_uuid = Uuid::new_v4();
    let changed = json!([
        "--resume",
        "x",
        "--fork-session",
        "--model",
        "opus",
        "--agent",
        "other"
    ]);
    launch_fork(&db, template_id, changed_uuid, &changed).await?;
    let changed_session = db.session(changed_uuid, "1 minute").await?;
    let sidecar = db
        .request(changed_session, "19 minutes 30 seconds", "title_sidecar")
        .await?;
    db.response(sidecar, 0, 200).await?;
    let first = db
        .request(changed_session, "19 minutes", "primary_first")
        .await?;
    db.response(first, 0, 80_000).await?;

    let hot_uuid = Uuid::new_v4();
    launch_fork(&db, template_id, hot_uuid, &forked_args()).await?;
    let hot_session = db.session(hot_uuid, "1 minute").await?;
    let hot = db
        .request(hot_session, "18 minutes", "primary_first")
        .await?;
    db.response(hot, 79_000, 1_000).await?;

    let findings = db.findings("cold_forks").await?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].resource, "rust: prefix_changed");
    assert_eq!(findings[0].evidence["forks"][0]["first_request_id"], first);
    assert_eq!(
        findings[0].evidence["forks"][0]["template_idle_seconds"],
        360
    );
    db.cleanup().await
}
