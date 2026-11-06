use chrono::NaiveDate;
use mike::store::persistence::lessons::skill_review_writes::{InstallReport, ReportedInstall};
use uuid::Uuid;

use super::skill_drafts::{Fixture, fixture};

async fn report_install(
    fx: &Fixture,
    folder: &str,
    watermark: i64,
    retired: bool,
) -> anyhow::Result<()> {
    let project_uuid: Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(fx.project_id)
            .fetch_one(fx.handle.pool())
            .await?;
    let day = NaiveDate::from_ymd_opt(2026, 10, 15)
        .ok_or_else(|| anyhow::anyhow!("fixture date must be valid"))?;
    let installs = [ReportedInstall {
        project_uuid,
        folder: folder.to_owned(),
        path: format!("/tmp/skill-drafts/.claude/skills/{folder}/SKILL.md"),
        draft_id: None,
        lesson_watermark: watermark,
        first_written: day,
        retired_at: retired.then_some(day),
        retired_reason: retired.then(|| "unloaded".to_owned()),
    }];
    fx.handle
        .record_skill_installs(InstallReport {
            machine_uuid: Uuid::from_u128(7),
            machine_name: "test-machine",
            installs: &installs,
        })
        .await?;
    Ok(())
}

async fn place(fx: &Fixture, lesson: i64, placement: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons
            SET placement_id = (SELECT id FROM memory.lesson_placements WHERE code = $2),
                placed_at = now()
          WHERE id = $1",
    )
    .bind(lesson)
    .bind(placement)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn lesson_in_a_live_installed_skill_is_withheld_and_a_newer_one_is_not() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let covered = fx.daily_lesson("Run migrate-check first.").await?;
    report_install(&fx, "database-migrations", covered, false).await?;
    let newer = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;

    let withheld = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(withheld.contains(&covered), "withheld: {withheld:?}");
    assert!(!withheld.contains(&newer), "withheld: {withheld:?}");
    fx.finish().await
}

#[tokio::test]
async fn lesson_whose_skill_is_retired_or_missing_is_not_withheld() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let lesson = fx.daily_lesson("Run migrate-check first.").await?;
    let none_installed = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(
        !none_installed.contains(&lesson),
        "withheld: {none_installed:?}"
    );

    report_install(&fx, "database-migrations", lesson, true).await?;
    let retired = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(!retired.contains(&lesson), "withheld: {retired:?}");

    report_install(&fx, "another-topic", lesson, false).await?;
    let other_folder = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(
        !other_folder.contains(&lesson),
        "withheld: {other_folder:?}"
    );
    fx.finish().await
}

#[tokio::test]
async fn placement_decides_whether_an_installed_skill_carries_the_lesson() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let in_skill = fx.daily_lesson("Run migrate-check first.").await?;
    let memory = fx
        .daily_lesson("Migrations are applied at startup.")
        .await?;
    let rule = fx.daily_lesson("Never edit an applied migration.").await?;
    place(&fx, memory, "memory").await?;
    place(&fx, rule, "rule").await?;
    report_install(&fx, "database-migrations", rule, false).await?;

    let withheld = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(withheld.contains(&in_skill), "withheld: {withheld:?}");
    assert!(!withheld.contains(&memory), "withheld: {withheld:?}");
    assert!(withheld.contains(&rule), "withheld: {withheld:?}");
    fx.finish().await
}

#[tokio::test]
async fn rule_placed_topic_lesson_stays_withheld_without_any_install() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let rule = fx.daily_lesson("Never edit an applied migration.").await?;
    place(&fx, rule, "rule").await?;
    let withheld = fx.handle.list_topic_lessons_withheld_from_content().await?;
    assert!(withheld.contains(&rule), "withheld: {withheld:?}");
    fx.finish().await
}
