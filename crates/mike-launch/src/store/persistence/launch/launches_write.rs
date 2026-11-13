use super::{LaunchOutcome, LaunchSurfaceRefs, NewLaunch};
use crate::store::persistence::PersistError;

fn missing_launch(id: i64, step: &str) -> PersistError {
    PersistError::DataInvariant(format!(
        "launch.launches id {id} does not exist, so its {step} was not recorded"
    ))
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn insert_launch(self, new: &NewLaunch) -> Result<i64, PersistError> {
        crate::metrics::time_db("launch_insert_launch", async move {
            let claude_args = serde_json::Value::from(new.claude_args.clone());
            let id = sqlx::query_file_scalar!(
                "src/store/persistence/launch/insert_launch.sql",
                new.session_uuid,
                new.backend,
                new.folder,
                new.repo_root,
                new.worktree,
                new.run_id,
                new.seat,
                new.template_id,
                new.model,
                new.agent,
                claude_args,
                new.prompt_present,
                new.brief_mode,
                new.brief_path,
                new.brief_text,
                new.requested_placement,
                new.split_direction,
                new.anchor_ref,
                new.remote_control,
                new.proxy_env,
                new.identity,
                new.brief_note_uuid,
                new.backend_reason,
            )
            .fetch_one(&self.pool)
            .await?;
            self.launches_changed(new.session_uuid);
            Ok(id)
        })
        .await
    }

    pub async fn record_launch_opened(
        self,
        id: i64,
        surface: &LaunchSurfaceRefs,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_record_launch_opened", async move {
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/record_launch_opened.sql",
                id,
                surface.workspace_ref,
                surface.tab_ref,
                surface.surface_ref,
                surface.pid,
                surface.actual_placement,
            )
            .fetch_optional(&self.pool)
            .await?;
            hit.map(|_| ()).ok_or_else(|| missing_launch(id, "opening"))
        })
        .await
    }

    pub async fn record_launch_outcome(
        self,
        id: i64,
        outcome: &LaunchOutcome,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_record_launch_outcome", async move {
            let stderr_text = outcome.stderr.as_ref().map(|s| s.text.as_str());
            let stderr_truncated = outcome.stderr.as_ref().is_some_and(|s| s.truncated);
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/record_launch_outcome.sql",
                id,
                outcome.result,
                outcome.reason,
                outcome.opened,
                outcome.delivered,
                outcome.confirmed,
                outcome.channel_check,
                stderr_text,
                stderr_truncated,
            )
            .fetch_optional(&self.pool)
            .await?;
            hit.map(|_| ()).ok_or_else(|| missing_launch(id, "outcome"))
        })
        .await
    }

    pub async fn record_launch_closed(
        self,
        id: i64,
        reason: &str,
        process_exited: Option<bool>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_record_launch_closed", async move {
            let run_id: Option<Option<String>> =
                sqlx::query_scalar("SELECT run_id FROM launch.launches WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?;
            let run_id = run_id.ok_or_else(|| missing_launch(id, "close"))?;
            if let Some(run_id) = run_id.filter(|run| run.starts_with("fleet-")) {
                return self
                    .record_fleet_launch_closure(id, &run_id, reason, process_exited)
                    .await;
            }
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/record_launch_closed.sql",
                id,
                reason,
                process_exited,
            )
            .fetch_optional(&self.pool)
            .await?;
            hit.map(|_| ()).ok_or_else(|| missing_launch(id, "close"))
        })
        .await
    }
}
