//! The rest of placement's tests.

use super::*;

#[tokio::test]
async fn a_draft_remembers_its_topic_and_a_shipped_topic_is_marked() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    lesson(
        &fx,
        "Check status after deploy.",
        "deploying-the-proxy",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Restart with start-proxy.sh.",
        "deploying-the-proxy",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Wait for the fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    lesson(
        &fx,
        "Use --during-a-fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    lesson(&fx, "Not a skill.", "other", Some("memory"), None).await?;

    let before = fx.handle.lessons().skill_purposes().await?;
    let seen: Vec<(String, i64, bool)> = before
        .iter()
        .map(|p| (p.topic.clone(), p.lessons, p.shipped))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("deploying-the-proxy".to_owned(), 2, false),
            ("proxy-deploy-steps".to_owned(), 2, false)
        ]
    );

    let target = fx
        .handle
        .lessons()
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("target"))?;
    let empty = json!([]);
    let id = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target, &[1, 2], &empty))
        .await?;
    let topic: Option<String> =
        sqlx::query_scalar("SELECT topic FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(topic.as_deref(), Some(target.topic.as_str()));

    sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
        .bind(id)
        .execute(fx.handle.pool())
        .await?;
    let after = fx.handle.lessons().skill_purposes().await?;
    let shipped: Vec<&str> = after
        .iter()
        .filter(|p| p.shipped)
        .map(|p| p.topic.as_str())
        .collect();
    assert_eq!(shipped, vec![target.topic.as_str()]);
    fx.finish().await
}

#[tokio::test]
async fn retopic_moves_only_skill_placed_lessons_of_the_named_topic() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let moved = lesson(
        &fx,
        "Wait for the fleet.",
        "proxy-deploy-steps",
        Some("skill"),
        None,
    )
    .await?;
    let stays = lesson(&fx, "A fact.", "proxy-deploy-steps", Some("memory"), None).await?;
    assert_eq!(
        fx.handle
            .lessons()
            .retopic_skill_lessons(fx.project_id, &[])
            .await?,
        0
    );
    let changed = fx
        .handle
        .lessons()
        .retopic_skill_lessons(
            fx.project_id,
            &[TopicMove {
                from: "proxy-deploy-steps".to_owned(),
                to: "deploying-the-proxy".to_owned(),
            }],
        )
        .await?;
    assert_eq!(changed, 1);
    let topic_of = |id: i64| {
        let pool = fx.handle.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT topic FROM memory.lessons WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
        }
    };
    assert_eq!(
        topic_of(moved).await?.as_deref(),
        Some("deploying-the-proxy")
    );
    assert_eq!(
        topic_of(stays).await?.as_deref(),
        Some("proxy-deploy-steps")
    );
    fx.finish().await
}
