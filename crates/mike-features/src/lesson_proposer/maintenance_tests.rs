use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::sync::Arc;
use std::time::Duration;

use super::{ProposerDeps, ProposerSettings, placement_pass, shape_pass};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::llm::limiter::CallLimiter;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::semantic::SemanticConfig;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::{PersistHandle, test_support};

const UNREACHABLE: &str = "http://127.0.0.1:1";

struct Fixture {
    db: test_support::TestDbHandle,
    handle: PersistHandle,
    project_id: i64,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), "/tmp/maintenance-budget").await?;
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
    }))
}

impl Fixture {
    async fn lesson(&self, text: &str) -> anyhow::Result<i64> {
        let row = self
            .handle
            .lessons()
            .create_lesson_with_topic(NewLessonWrite {
                project_id: self.project_id,
                kind: "convention",
                text,
                topic: Some("deploy"),
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?;
        Ok(row.id)
    }

    async fn counters(&self) -> anyhow::Result<(i64, i64)> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT consumed, maintenance_consumed
               FROM memory.lesson_proposer_daily_budget
              WHERE day = (now() AT TIME ZONE 'UTC')::date",
        )
        .fetch_optional(self.handle.pool())
        .await?;
        Ok(row.unwrap_or((0, 0)))
    }

    async fn refusal(&self, pass: &str) -> anyhow::Result<Option<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT reason, cap FROM memory.lesson_maintenance_refusals WHERE pass = $1",
        )
        .bind(pass)
        .fetch_optional(self.handle.pool())
        .await?)
    }
}

fn client(limiter: Arc<CallLimiter>) -> anyhow::Result<OpenRouterClient> {
    Ok(OpenRouterClient::new_with_limiter(
        "test-key".to_owned(),
        "test-model".to_owned(),
        Some(UNREACHABLE.to_owned()),
        Some(Duration::from_secs(2)),
        limiter,
    )?
    .with_role(LlmRole::LessonProposer))
}

fn open_limiter() -> Arc<CallLimiter> {
    Arc::new(CallLimiter::new(
        100,
        Duration::from_secs(60),
        100,
        Duration::from_secs(3600),
    ))
}

fn exhausted_limiter() -> Arc<CallLimiter> {
    let limiter = open_limiter();
    limiter.exhaust_rate();
    limiter
}

fn embedding() -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        url: "http://127.0.0.1:1".to_owned(),
        model: "test".to_owned(),
        chunk_max_chars: 1500,
    })
}

#[tokio::test]
async fn placement_pass_counts_on_the_maintenance_counter_and_leaves_the_proposer_counter_alone()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    fx.lesson("Restart with start-proxy.sh.").await?;
    let llm = client(open_limiter())?;
    let embedding = embedding();
    let semantic = SemanticConfig::default();
    let settings = ProposerSettings {
        max_per_day: 0,
        ..ProposerSettings::default()
    };
    let deps = ProposerDeps {
        persist: &fx.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
    };
    let report = placement_pass::run_pass(&deps).await;
    assert!(!report.budget_exhausted, "{report:?}");
    assert_eq!(report.failed, 1, "{report:?}");
    assert_eq!(fx.counters().await?, (0, 1));
    assert_eq!(fx.refusal("placement").await?, None);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn placement_pass_refused_by_the_maintenance_cap_records_the_refusal() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let lesson = fx.lesson("Restart with start-proxy.sh.").await?;
    let llm = client(open_limiter())?;
    let embedding = embedding();
    let semantic = SemanticConfig::default();
    let settings = ProposerSettings {
        maintenance_max_per_day: 0,
        ..ProposerSettings::default()
    };
    let deps = ProposerDeps {
        persist: &fx.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
    };
    let report = placement_pass::run_pass(&deps).await;
    assert!(report.budget_exhausted);
    assert_eq!(report.failed, 0);
    assert_eq!(
        fx.refusal("placement").await?,
        Some(("daily_budget".to_owned(), 0))
    );
    assert_eq!(fx.counters().await?, (0, 0));
    let waiting = fx.handle.lessons().unplaced_lessons(10).await?;
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].id, lesson);
    let seen = fx.handle.lessons().recent_maintenance_refusals().await?;
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].pass, "placement");
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn placement_pass_refused_by_the_call_limiter_records_a_provider_refusal()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    fx.lesson("Restart with start-proxy.sh.").await?;
    let llm = client(exhausted_limiter())?;
    let embedding = embedding();
    let semantic = SemanticConfig::default();
    let settings = ProposerSettings::default();
    let deps = ProposerDeps {
        persist: &fx.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
    };
    let report = placement_pass::run_pass(&deps).await;
    assert!(report.budget_exhausted);
    assert_eq!(
        fx.refusal("placement").await?,
        Some(("provider_limit".to_owned(), 400))
    );
    assert_eq!(fx.counters().await?, (0, 0));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn shape_pass_refused_by_the_maintenance_cap_records_the_refusal() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let long = "Restart the proxy with start-proxy.sh after every deploy. ".repeat(20);
    fx.lesson(&long).await?;
    let llm = client(open_limiter())?;
    let embedding = embedding();
    let semantic = SemanticConfig::default();
    let settings = ProposerSettings {
        maintenance_max_per_day: 0,
        ..ProposerSettings::default()
    };
    let deps = ProposerDeps {
        persist: &fx.handle,
        llm: &llm,
        embedding: &embedding,
        semantic: &semantic,
        settings: &settings,
    };
    let report = shape_pass::run_pass(&deps).await;
    assert!(report.flagged >= 1, "{report:?}");
    assert!(report.budget_exhausted, "{report:?}");
    assert_eq!(
        fx.refusal("shape").await?,
        Some(("daily_budget".to_owned(), 0))
    );
    assert_eq!(fx.counters().await?, (0, 0));
    fx.db.cleanup().await?;
    Ok(())
}
