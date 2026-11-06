use mike::lesson_proposer::skill_eval::run_pass;
use mike::llm::{OpenRouterClient, reset_global_for_tests};
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::skill_drafts::{draft, fixture};
use super::skill_eval_pass::{
    FOLDER, Scripted, insert_evaluable, passes_until_decided, serve, stored,
};

#[tokio::test]
async fn a_draft_the_reviewer_finds_already_covered_is_rejected_with_the_named_item()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let (_server, client) = serve(Scripted {
        loads_skill: true,
        loads_when_patched: false,
        decision: "nothing_to_save",
    })
    .await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "rejected");
    assert_eq!(eval["review"]["decision"], "nothing_to_save");
    assert_eq!(eval["review"]["overlap"], "deploy-and-restart");
    assert!(
        eval["verdict"]["reasons"][0]
            .as_str()
            .is_some_and(|r| r.contains("already covered by deploy-and-restart"))
    );
    fx.finish().await
}

#[tokio::test]
async fn a_draft_that_belongs_in_an_existing_skill_is_rejected_with_the_place_and_the_skill()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let (_server, client) = serve(Scripted {
        loads_skill: true,
        loads_when_patched: false,
        decision: "wrong_place",
    })
    .await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "rejected");
    assert_eq!(eval["review"]["decision"], "wrong_place");
    assert_eq!(eval["review"]["belongs_in"], "existing_skill");
    assert_eq!(eval["review"]["overlap"], "ui-design");
    let evidence: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT review_overlap, review_belongs_in FROM memory.skill_evidence_v WHERE folder = $1",
    )
    .bind(FOLDER)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(
        evidence,
        (
            Some("ui-design".to_owned()),
            Some("existing_skill".to_owned())
        )
    );
    fx.finish().await
}

#[tokio::test]
async fn a_failing_model_is_recorded_and_the_draft_waits() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "test/model".to_owned(),
        Some(server.uri()),
        None,
    )?;
    reset_global_for_tests();

    let report = run_pass(&fx.handle, &client).await;
    assert!(report.error.is_some(), "{report:?}");
    let attempts: i32 = sqlx::query_scalar(
        "SELECT attempts FROM memory.skill_draft_eval_failures WHERE draft_id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(attempts, 1);
    assert_eq!(stored(&fx, id).await?.0, "evaluating");

    reset_global_for_tests();
    let waiting = run_pass(&fx.handle, &client).await;
    assert!(
        waiting.draft_id.is_none(),
        "a failed draft must wait: {waiting:?}"
    );
    fx.finish().await
}

#[tokio::test]
async fn a_draft_without_trigger_queries_is_rejected_at_once() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: FOLDER.to_owned(),
        topic: FOLDER.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: 50,
    };
    let empty = json!([]);
    let id = fx
        .handle
        .insert_skill_draft(draft(&target, &[1, 2], &empty))
        .await?;
    let (_server, client) = serve(Scripted::approving()).await?;
    reset_global_for_tests();

    let report = run_pass(&fx.handle, &client).await;
    assert_eq!(report.accepted, Some(false));
    let (status, eval, _) = stored(&fx, id).await?;
    assert_eq!(status, "rejected");
    assert!(eval.is_some_and(|e| {
        e["error"]
            .as_str()
            .is_some_and(|m| m.contains("trigger_queries is empty"))
    }));
    fx.finish().await
}
