use mike::store::persistence::lessons::placement_proposals::PlacementDecision;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use uuid::Uuid;

use super::skill_drafts::{Fixture, fixture};

async fn place(fx: &Fixture, lesson_id: i64, code: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons
            SET placement_id = (SELECT id FROM memory.lesson_placements WHERE code = $2),
                placement_reason = 'a standing constraint on these files',
                placed_at = now()
          WHERE id = $1",
    )
    .bind(lesson_id)
    .bind(code)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

async fn project_uuid(fx: &Fixture) -> anyhow::Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(fx.project_id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn undecided(fx: &Fixture, project: Uuid) -> anyhow::Result<i64> {
    let projects = fx.handle.lessons().list_proposal_projects().await?;
    let row = projects
        .iter()
        .find(|p| p.project_uuid == project)
        .ok_or_else(|| anyhow::anyhow!("a project with a rule proposal must be listed"))?;
    Ok(row.undecided)
}

#[tokio::test]
async fn rule_and_agent_lessons_list_as_proposals_and_skill_lessons_do_not() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let rule = fx.daily_lesson("Keep the tag pin in Cargo.toml.").await?;
    let agent = fx
        .daily_lesson("The reviewer reads the changelog first.")
        .await?;
    let skill = fx
        .daily_lesson("Run migrate-check before a deploy.")
        .await?;
    place(&fx, rule, "rule").await?;
    place(&fx, agent, "agent").await?;
    let project = project_uuid(&fx).await?;

    let rows = fx
        .handle
        .lessons()
        .list_placement_proposals(project)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.lesson_id).collect();
    assert_eq!(ids, vec![rule, agent]);
    assert!(!ids.contains(&skill));
    assert_eq!(rows[0].placement, "rule");
    assert_eq!(rows[1].placement, "agent");
    assert_eq!(undecided(&fx, project).await?, 2);

    fx.finish().await
}

#[tokio::test]
async fn decision_is_stored_replaced_and_cleared() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let rule = fx.daily_lesson("Keep the tag pin in Cargo.toml.").await?;
    place(&fx, rule, "rule").await?;
    let project = project_uuid(&fx).await?;

    assert!(
        fx.handle
            .lessons()
            .set_placement_decision(rule, PlacementDecision::Accepted)
            .await?
    );
    assert_eq!(undecided(&fx, project).await?, 0);
    let rows = fx
        .handle
        .lessons()
        .list_placement_proposals(project)
        .await?;
    assert_eq!(rows[0].decision.as_deref(), Some("accepted"));

    assert!(
        fx.handle
            .lessons()
            .set_placement_decision(rule, PlacementDecision::Declined)
            .await?
    );
    let rows = fx
        .handle
        .lessons()
        .list_placement_proposals(project)
        .await?;
    assert_eq!(rows[0].decision.as_deref(), Some("declined"));

    assert!(
        fx.handle
            .lessons()
            .set_placement_decision(rule, PlacementDecision::Cleared)
            .await?
    );
    assert_eq!(undecided(&fx, project).await?, 1);

    fx.finish().await
}

#[tokio::test]
async fn deciding_a_lesson_that_is_not_a_proposal_finds_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let skill = fx
        .daily_lesson("Run migrate-check before a deploy.")
        .await?;

    assert!(
        !fx.handle
            .lessons()
            .set_placement_decision(skill, PlacementDecision::Accepted)
            .await?
    );
    assert!(
        !fx.handle
            .lessons()
            .set_placement_decision(i64::MAX, PlacementDecision::Declined)
            .await?
    );

    fx.finish().await
}
