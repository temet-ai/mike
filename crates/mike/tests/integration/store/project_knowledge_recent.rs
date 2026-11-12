use super::knowledge_search_fixture::{Fixture, query, references};
use chrono::{DateTime, TimeZone, Utc};
use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use serde_json::Value;

struct Http {
    base: String,
    client: reqwest::Client,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Http {
    async fn serve(fx: &Fixture) -> anyhow::Result<Self> {
        let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        state.persist = Some(fx.handle.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let router = mike::app::inspect_router_on(state, listener.local_addr()?);
        let base = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            base,
            client: reqwest::Client::new(),
            server,
        })
    }

    async fn search(
        &self,
        fx: &Fixture,
        params: &[(&str, &str)],
    ) -> anyhow::Result<(reqwest::StatusCode, Value)> {
        let mut pairs = vec![("project", fx.path.as_str())];
        pairs.extend_from_slice(params);
        let response = self
            .client
            .get(format!("{}/api/knowledge/search", self.base))
            .query(&pairs)
            .send()
            .await?;
        Ok((response.status(), response.json().await?))
    }

    fn stop(self) {
        self.server.abort();
    }
}

async fn date_turn(fx: &Fixture, turn: i64, day: u32) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory.operator_turns SET created_at = $2 WHERE id = $1")
        .bind(turn)
        .bind(Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).single())
        .execute(fx.handle.pool())
        .await?;
    Ok(())
}

fn refs(answer: &Value) -> Vec<String> {
    references(&answer["items"].as_array().cloned().unwrap_or_default())
}

#[tokio::test]
async fn search_order_recent_returns_the_pool_newest_first_and_leaves_out_a_new_row_that_does_not_match()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let oldest = fx.turn(&session, "Tune the gibbon cache", "Tuned").await?;
    let newest = fx.turn(&session, "Size the gibbon cache", "Sized").await?;
    let middle = fx.turn(&session, "Warm the gibbon cache", "Warmed").await?;
    let unrelated = fx
        .turn(&session, "Order lunch for Friday", "Ordered")
        .await?;
    for (turn, day) in [(oldest, 3), (newest, 20), (middle, 10), (unrelated, 28)] {
        date_turn(&fx, turn, day).await?;
    }
    let http = Http::serve(&fx).await?;

    let (status, answer) = http
        .search(&fx, &[("query", "gibbon"), ("order", "recent")])
        .await?;

    assert!(status.is_success(), "{answer}");
    assert_eq!(
        refs(&answer),
        vec![
            format!("turn:{newest}"),
            format!("turn:{middle}"),
            format!("turn:{oldest}")
        ]
    );
    assert_eq!(answer["rerank"]["state"], "off");
    assert!(
        answer["rerank"]["detail"]
            .as_str()
            .is_some_and(|text| text.contains("most recent activity"))
    );
    http.stop();
    fx.finish().await
}

#[tokio::test]
async fn search_order_recent_pages_from_the_newest_first_list() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let session = fx.session().await?;
    let mut turns = Vec::new();
    for (prompt, day) in [
        ("First note on the ocelot", 2),
        ("Second note on the ocelot", 9),
        ("Third note on the ocelot", 16),
    ] {
        let turn = fx.turn(&session, prompt, "Noted").await?;
        date_turn(&fx, turn, day).await?;
        turns.push(format!("turn:{turn}"));
    }
    let http = Http::serve(&fx).await?;

    let (_, first) = http
        .search(
            &fx,
            &[("query", "ocelot"), ("order", "recent"), ("limit", "2")],
        )
        .await?;
    let (_, second) = http
        .search(
            &fx,
            &[
                ("query", "ocelot"),
                ("order", "recent"),
                ("limit", "2"),
                ("offset", "2"),
            ],
        )
        .await?;

    assert_eq!(refs(&first), vec![turns[2].clone(), turns[1].clone()]);
    assert_eq!(first["has_more"], true);
    assert_eq!(refs(&second), vec![turns[0].clone()]);
    assert_eq!(second["has_more"], false);
    http.stop();
    fx.finish().await
}

#[tokio::test]
async fn search_order_other_than_relevance_or_recent_is_a_400_naming_both() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let http = Http::serve(&fx).await?;

    let (status, answer) = http
        .search(&fx, &[("query", "gibbon"), ("order", "newest")])
        .await?;
    let (relevance, _) = http
        .search(&fx, &[("query", "gibbon"), ("order", "relevance")])
        .await?;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    let message = answer["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("`relevance`") && message.contains("`recent`"),
        "{answer}"
    );
    assert!(message.contains("newest"), "{answer}");
    assert!(relevance.is_success());
    http.stop();
    fx.finish().await
}

fn midnight(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, 0, 0, 0)
        .single()
        .unwrap_or_default()
}

fn moment(item: &Value) -> anyhow::Result<DateTime<Utc>> {
    let text = item["updated_at"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("updated_at missing from {item}"))?;
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

async fn work_thread(
    fx: &Fixture,
    slug: &str,
    (parsed, updated): (DateTime<Utc>, DateTime<Utc>),
    entry_days: &[u32],
) -> anyhow::Result<()> {
    let thread: i64 = sqlx::query_scalar(
        "INSERT INTO work.threads (project_id, slug, title, source_path, parsed_at, updated_at)
         VALUES ($1, $2, 'Quokka rollout', 'worklog.md', $3, $4) RETURNING id",
    )
    .bind(fx.project)
    .bind(slug)
    .bind(parsed)
    .bind(updated)
    .fetch_one(fx.handle.pool())
    .await?;
    for (ord, day) in entry_days.iter().enumerate() {
        sqlx::query(
            "INSERT INTO work.thread_log (thread_id, ord, entry_date, summary, kind, body)
             VALUES ($1, $2, make_date(2026, 10, $3), 'Quokka checkpoint', 'handoff', '')",
        )
        .bind(thread)
        .bind(i32::try_from(ord + 1)?)
        .bind(i32::try_from(*day)?)
        .execute(fx.handle.pool())
        .await?;
    }
    Ok(())
}

async fn thread_time(fx: &Fixture, slug: &str) -> anyhow::Result<DateTime<Utc>> {
    let items = fx
        .search(KnowledgeQuery {
            kind: Some("work_thread".into()),
            ..query("quokka")
        })
        .await?;
    let reference = format!("work_thread:{slug}");
    let item = items
        .iter()
        .find(|item| item["reference"] == reference.as_str())
        .ok_or_else(|| anyhow::anyhow!("{reference} missing from {items:?}"))?;
    moment(item)
}

#[tokio::test]
async fn search_work_thread_imported_and_untouched_reports_its_newest_entry_date()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let imported = Utc
        .with_ymd_and_hms(2026, 10, 27, 5, 58, 3)
        .single()
        .unwrap_or_default();
    work_thread(&fx, "imported", (imported, imported), &[1, 19, 7]).await?;

    assert_eq!(thread_time(&fx, "imported").await?, midnight(19));
    fx.finish().await
}

#[tokio::test]
async fn search_work_thread_changed_after_import_reports_its_updated_at() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let imported = Utc
        .with_ymd_and_hms(2026, 10, 27, 5, 58, 3)
        .single()
        .unwrap_or_default();
    let changed = Utc
        .with_ymd_and_hms(2026, 11, 2, 8, 30, 0)
        .single()
        .unwrap_or_default();
    work_thread(&fx, "changed", (imported, changed), &[19]).await?;

    assert_eq!(thread_time(&fx, "changed").await?, changed);
    fx.finish().await
}

#[tokio::test]
async fn search_work_thread_without_entries_reports_its_updated_at() -> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let imported = Utc
        .with_ymd_and_hms(2026, 10, 27, 5, 58, 3)
        .single()
        .unwrap_or_default();
    work_thread(&fx, "bare", (imported, imported), &[]).await?;

    assert_eq!(thread_time(&fx, "bare").await?, imported);
    fx.finish().await
}

#[tokio::test]
async fn search_thread_entry_reports_its_entry_date_when_imported_and_its_stored_time_when_live()
-> anyhow::Result<()> {
    let Some(fx) = Fixture::open().await? else {
        return Ok(());
    };
    let now = Utc::now();
    work_thread(&fx, "entries", (now, now), &[19]).await?;
    let live: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO work.thread_log (thread_id, ord, entry_date, summary, kind, body)
         SELECT id, 2, current_date, 'Quokka live checkpoint', 'handoff', ''
           FROM work.threads WHERE project_id = $1 AND slug = 'entries'
         RETURNING created_at",
    )
    .bind(fx.project)
    .fetch_one(fx.handle.pool())
    .await?;

    let items = fx
        .search(KnowledgeQuery {
            kind: Some("thread_entry".into()),
            ..query("quokka")
        })
        .await?;

    let mut found = Vec::new();
    for item in &items {
        found.push((
            item["title"].as_str().unwrap_or_default().to_owned(),
            moment(item)?,
        ));
    }
    found.sort();
    assert_eq!(
        found,
        vec![
            ("Quokka checkpoint".to_owned(), midnight(19)),
            ("Quokka live checkpoint".to_owned(), live)
        ]
    );
    fx.finish().await
}
