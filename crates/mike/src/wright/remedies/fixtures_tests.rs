use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::Utc;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::Owners;
use crate::config::{FilterWriteGuard, SettingsWriteGuard};
use crate::filter::FilterConfig;
use crate::filter::service::{FilterRuleCreateInput, FilterService};
use crate::settings::SettingsOverlay;
use crate::settings::service::SettingsService;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};
use crate::store::persistence::wright::RecordedRun;
use crate::store::persistence::wright::remedies::{ActionFilter, ActionRow, RemedyRow};
use crate::wright::watch::{WatchContext, run_check};
use crate::wright::{CheckOutcome, CheckRun, Finding};

pub(super) struct StuckMail {
    pub(super) recipient: Uuid,
    pub(super) to: i64,
    pub(super) sender: i64,
    pub(super) messages: Vec<Uuid>,
}

pub(super) struct Env {
    pub(super) db: TestDbHandle,
    pub(super) owners: Owners,
    pub(super) project: i64,
}

pub(super) async fn owners_for(url: &str) -> anyhow::Result<Owners> {
    let persist = PersistHandle::connect(url).await?;
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    let settings = SettingsService::reconcile_on_startup(
        overlay,
        Arc::new(Mutex::new(SettingsWriteGuard)),
        persist.clone(),
    )
    .await?;
    let filters = FilterService::reconcile_on_startup(
        Arc::new(ArcSwap::from_pointee(FilterConfig::default())),
        Arc::new(Mutex::new(FilterWriteGuard)),
        persist.clone(),
    )
    .await?;
    Ok(Owners {
        persist,
        settings: Some(settings),
        filters: Some(filters),
    })
}

impl Env {
    pub(super) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let owners = owners_for(&db.database_url).await?;
        let project = seed_project(owners.persist.pool(), "/wright/remedies").await?;
        Ok(Self {
            db,
            owners,
            project,
        })
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }

    pub(super) fn pool(&self) -> &sqlx::PgPool {
        self.owners.persist.pool()
    }

    pub(super) async fn backdate(&self, action: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE wright.actions SET updated_at = now() - interval '10 minutes' WHERE id = $1",
        )
        .bind(action)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn limits(
        &self,
        remedy: &str,
        daily: i32,
        resources: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE wright.remedies SET daily_limit = $2, max_resources = $3 WHERE name = $1",
        )
        .bind(remedy)
        .bind(daily)
        .bind(resources)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn ineffective_runs(&self, remedy: &str, runs: i32) -> anyhow::Result<()> {
        sqlx::query("UPDATE wright.remedies SET ineffective_runs = $2 WHERE name = $1")
            .bind(remedy)
            .bind(runs)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn own_session(&self, uuid: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS wright.test_own_sessions (session_uuid uuid NOT NULL)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE OR REPLACE FUNCTION wright.wright_session_uuids() RETURNS SETOF uuid
             LANGUAGE sql STABLE AS $$ SELECT session_uuid FROM wright.test_own_sessions $$",
        )
        .execute(self.pool())
        .await?;
        sqlx::query("INSERT INTO wright.test_own_sessions (session_uuid) VALUES ($1)")
            .bind(uuid)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn rename_resource(&self, from: &str, to: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE wright.problems SET resource = $2 WHERE resource = $1")
            .bind(from)
            .bind(to)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn unknown_run(&self, check: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO wright.check_runs (check_name, source, cadence, started_at, outcome, error)
             VALUES ($1, 'capture_db', 'event', now(), 'unknown', 'no answer')",
        )
        .bind(check)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn session(&self, uuid: Uuid, last_seen_ago: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, now() - interval '2 days', now() - $3::interval)
             RETURNING id",
        )
        .bind(uuid)
        .bind(self.project)
        .bind(last_seen_ago)
        .fetch_one(self.pool())
        .await?)
    }

    pub(super) async fn fleet_seat(&self, seat: Uuid, run_id: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO launch.launches (session_uuid, backend, folder, run_id, seat, opened)
             VALUES ($1, 'none', '/wright/remedies', $2, 'worker-1', true)",
        )
        .bind(seat)
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn running_fleet(
        &self,
        run_id: &str,
        coordinator: Option<Uuid>,
    ) -> anyhow::Result<()> {
        let definition = match coordinator {
            Some(uuid) => json!({"coordinator": {"session": uuid.to_string()}}),
            None => json!({}),
        };
        sqlx::query(
            "INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'none') ON CONFLICT DO NOTHING",
        )
        .bind(run_id)
        .execute(self.pool())
        .await?;
        sqlx::query(
            "INSERT INTO launch.fleets (slug, project_path, state, definition, run_id, started_at)
             VALUES ('wright-remedies', '/wright/remedies', 'running', $1, $2, now())",
        )
        .bind(definition)
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn mail(&self, to: i64, from: Option<i64>) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body, queued_at)
             VALUES (gen_random_uuid(), $1, $2, 'hello', now() - interval '2 hours')
             RETURNING message_uuid",
        )
        .bind(to)
        .bind(from)
        .fetch_one(self.pool())
        .await?)
    }

    pub(super) async fn stuck_mail(&self, messages: usize) -> anyhow::Result<StuckMail> {
        let recipient = Uuid::new_v4();
        let to = self.session(recipient, "1 minute").await?;
        let sender = self.session(Uuid::new_v4(), "1 minute").await?;
        let mut queued = Vec::with_capacity(messages);
        for _ in 0..messages {
            queued.push(self.mail(to, Some(sender)).await?);
        }
        Ok(StuckMail {
            recipient,
            to,
            sender,
            messages: queued,
        })
    }

    pub(super) async fn notices_to(&self, sender: i64) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM mailbox.messages WHERE to_session_id = $1 AND from_session_id IS NULL",
        )
        .bind(sender)
        .fetch_one(self.pool())
        .await?)
    }

    pub(super) async fn read_all(&self, to: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE mailbox.messages SET consumed_at = now() WHERE to_session_id = $1")
            .bind(to)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn watch(&self, check: &str) -> anyhow::Result<RecordedRun> {
        let row = self
            .owners
            .persist
            .web()
            .wright_check(check)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no check named {check}"))?;
        let ctx = WatchContext {
            persist: self.owners.persist.clone(),
            log_dir: None,
            machine: None,
            repo_root: None,
        };
        let (outcome, recorded) = run_check(&row, &ctx).await?;
        if let CheckOutcome::Unknown(reason) = outcome {
            anyhow::bail!("{check} could not run: {reason}");
        }
        Ok(recorded)
    }

    pub(super) async fn record(
        &self,
        check: &str,
        resources: &[String],
    ) -> anyhow::Result<RecordedRun> {
        let findings = resources
            .iter()
            .map(|resource| Finding {
                scope: "test".to_owned(),
                resource: resource.clone(),
                evidence: json!({"resource": resource}),
            })
            .collect();
        Ok(self
            .owners
            .persist
            .web()
            .record_wright_run(&CheckRun {
                check,
                source: "capture_db",
                cadence: "event",
                started_at: Utc::now(),
                outcome: &CheckOutcome::Complete(findings),
                machine: None,
            })
            .await?)
    }

    pub(super) async fn remedy_row(
        &self,
        name: &str,
        kind: &str,
        action: &str,
        check: &str,
        params: Value,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO wright.remedies (name, kind, action, check_name, params, daily_limit, max_resources, measure_after_passes)
             VALUES ($1, $2, $3, $4, $5, 20, 5, 1)",
        )
        .bind(name)
        .bind(kind)
        .bind(action)
        .bind(check)
        .bind(params)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(super) async fn only(&self, remedy: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE wright.remedies SET enabled = (name = $1)")
            .bind(remedy)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(super) async fn remedy(&self, name: &str) -> anyhow::Result<RemedyRow> {
        self.owners
            .persist
            .web()
            .wright_remedies(Some(name))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no remedy named {name}"))
    }

    pub(super) async fn actions(&self, remedy: &str) -> anyhow::Result<Vec<ActionRow>> {
        let filter = ActionFilter {
            limit: 500,
            ..ActionFilter::default()
        };
        Ok(self
            .owners
            .persist
            .web()
            .wright_actions(&filter)
            .await?
            .into_iter()
            .filter(|row| row.remedy == remedy)
            .collect())
    }

    pub(super) async fn filter_rule(&self, value: &str) -> anyhow::Result<i64> {
        let filters = self
            .owners
            .filters
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no filter service"))?;
        let created = filters
            .create(FilterRuleCreateInput {
                kind: "text_substring".to_owned(),
                value: value.to_owned(),
                action: "append".to_owned(),
                text: Some("wright test".to_owned()),
                trigger_token: None,
                inject_text: None,
                enabled: true,
                project_path: None,
                style_override: false,
                title: None,
            })
            .await?;
        Ok(created.rule.id)
    }
}
