use super::*;

#[tokio::test]
async fn the_project_path_of_a_draft_is_looked_up_by_project_id() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(
        fx.handle
            .lessons()
            .skill_draft_project_path(fx.project_id)
            .await?
            .as_deref(),
        Some("/tmp/skill-drafts")
    );
    assert_eq!(
        fx.handle.lessons().skill_draft_project_path(-1).await?,
        None
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn eval_queries_on_a_closed_pool_are_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = target_for(&fx, "first", 10);
    fx.handle.pool().close().await;
    assert!(
        fx.handle
            .lessons()
            .skill_draft_project_path(1)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .claim_skill_draft_for_eval()
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .skill_draft_siblings(fx.project_id, "x", 5)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .save_skill_draft_progress(1, &json!({}))
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .finish_skill_draft_eval(1, true, &json!({}), None)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .fail_skill_draft_eval(1, "x", 3, EVALUATOR_VERSION)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .previous_rejected_skill_draft(&target)
            .await
            .is_err()
    );
    fx.db.cleanup().await
}
