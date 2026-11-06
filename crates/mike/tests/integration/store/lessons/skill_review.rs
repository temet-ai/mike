use chrono::NaiveDate;
use mike::store::persistence::lessons::skill_review_writes::{
    InstallReport, ReportedInstall, SkillReview,
};
use serde_json::json;
use uuid::Uuid;

use super::skill_drafts::{Fixture, draft, fixture};

async fn accepted_draft(fx: &Fixture) -> anyhow::Result<i64> {
    fx.daily_lesson("Run migrate-check first.").await?;
    fx.daily_lesson("Commit migrations with their code.")
        .await?;
    let target = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a topic with two daily lessons must be a target"))?;
    let ids: Vec<i64> = fx
        .handle
        .skill_draft_lessons(&target)
        .await?
        .iter()
        .map(|l| l.id)
        .collect();
    let empty = json!([]);
    let id = fx
        .handle
        .insert_skill_draft(draft(&target, &ids, &empty))
        .await?;
    sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
        .bind(id)
        .execute(fx.handle.pool())
        .await?;
    Ok(id)
}

async fn project_uuid(fx: &Fixture) -> anyhow::Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(fx.project_id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn waiting(fx: &Fixture, project: Uuid) -> anyhow::Result<i64> {
    let projects = fx.handle.list_skill_projects().await?;
    let row = projects
        .iter()
        .find(|p| p.project_uuid == project)
        .ok_or_else(|| anyhow::anyhow!("a project with a draft must be listed"))?;
    Ok(row.waiting)
}

#[tokio::test]
async fn project_without_policy_asks_first_and_approval_clears_the_wait() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = accepted_draft(&fx).await?;
    let project = project_uuid(&fx).await?;

    assert_eq!(
        fx.handle.skill_project_mode(project).await?,
        Some(("ask".to_owned(), false))
    );
    assert_eq!(waiting(&fx, project).await?, 1);

    assert!(
        fx.handle
            .set_skill_draft_review(id, SkillReview::Approved)
            .await?
    );
    assert_eq!(waiting(&fx, project).await?, 0);
    let folders = fx.handle.list_project_skill_folders(project).await?;
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].approved_draft_id, Some(id));
    assert_eq!(folders[0].review.as_deref(), Some("approved"));

    assert!(
        fx.handle
            .set_skill_draft_review(id, SkillReview::Cleared)
            .await?
    );
    assert!(fx.handle.set_skill_policy(project, "auto", "test").await?);
    assert_eq!(
        fx.handle.skill_project_mode(project).await?,
        Some(("auto".to_owned(), true))
    );
    assert_eq!(
        waiting(&fx, project).await?,
        0,
        "an auto project never waits on the user"
    );
    assert!(
        !fx.handle
            .set_skill_draft_review(i64::MAX, SkillReview::Approved)
            .await?
    );
    fx.finish().await
}

#[tokio::test]
async fn edit_keeps_the_authored_text_once_and_retire_restore_round_trips() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = accepted_draft(&fx).await?;
    let project = project_uuid(&fx).await?;

    assert!(fx.handle.edit_skill_draft(id, "first edit").await?);
    assert!(fx.handle.edit_skill_draft(id, "second edit").await?);
    let detail = fx
        .handle
        .skill_draft_detail(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("draft must load"))?;
    assert_eq!(detail.skill_md, "second edit");
    assert_eq!(
        detail.authored_skill_md.as_deref(),
        Some("---\nname: database-migrations\n---\nbody")
    );
    assert!(detail.edited_at.is_some());
    assert_eq!(
        fx.handle
            .skill_draft_source_lessons(&detail.lesson_ids)
            .await?
            .len(),
        2
    );

    assert!(
        fx.handle
            .retire_skill_folder(project, &detail.folder)
            .await?
    );
    assert!(
        fx.handle
            .retire_skill_folder(project, &detail.folder)
            .await?,
        "retiring twice is not an error"
    );
    assert_eq!(waiting(&fx, project).await?, 0);
    let folders = fx.handle.list_project_skill_folders(project).await?;
    assert!(folders[0].retired_by_user_at.is_some());
    assert!(
        fx.handle
            .restore_skill_folder(project, &detail.folder)
            .await?
    );
    assert!(
        !fx.handle
            .restore_skill_folder(project, &detail.folder)
            .await?
    );
    fx.finish().await
}

fn install(project: Uuid, folder: &str, draft_id: Option<i64>) -> ReportedInstall {
    ReportedInstall {
        project_uuid: project,
        folder: folder.to_owned(),
        path: format!("/work/proj/.claude/skills/{folder}/SKILL.md"),
        draft_id,
        lesson_watermark: 9,
        first_written: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap_or_default(),
        retired_at: None,
        retired_reason: None,
    }
}

#[tokio::test]
async fn installs_report_replaces_a_machines_rows_and_feeds_the_folder_view() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = accepted_draft(&fx).await?;
    let project = project_uuid(&fx).await?;
    let machine = Uuid::new_v4();
    let first = [
        install(project, "database-migrations", Some(id)),
        install(project, "old-topic", None),
    ];
    let report = InstallReport {
        machine_uuid: machine,
        machine_name: "test-mac",
        installs: &first,
    };
    assert_eq!(fx.handle.record_skill_installs(report).await?, 2);

    let folders = fx.handle.list_project_skill_folders(project).await?;
    let names: Vec<&str> = folders.iter().map(|f| f.folder.as_str()).collect();
    assert_eq!(names, vec!["database-migrations", "old-topic"]);
    assert!(folders[0].live);
    assert_eq!(folders[0].installs[0]["machine_name"], "test-mac");
    assert!(folders[1].draft_id.is_none());

    let second = [install(project, "database-migrations", Some(id))];
    let report = InstallReport {
        machine_uuid: machine,
        machine_name: "test-mac",
        installs: &second,
    };
    fx.handle.record_skill_installs(report).await?;
    let folders = fx.handle.list_project_skill_folders(project).await?;
    assert_eq!(
        folders.len(),
        1,
        "a folder this machine no longer reports is dropped"
    );
    fx.finish().await
}
