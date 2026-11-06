use mike::lesson_proposer::skill_eval::EVALUATOR_VERSION;
use serde_json::{Value, json};

use super::skill_drafts::fixture;
use super::skill_eval_listing::{listing_body, request, session};
use super::skill_eval_pass::{
    Scripted, insert_evaluable, message, passes_until_decided, serve, stored,
};

const REAL_SKILLS: usize = 80;
const SAMPLE_SIZE: usize = 60;

#[tokio::test]
async fn trigger_queries_list_the_skills_the_projects_sessions_saw_beside_the_draft()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    let names: Vec<String> = (0..REAL_SKILLS)
        .map(|i| format!("real-skill-{i}"))
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let seat = session(&fx, fx.project_id).await?;
    let carried = request(&fx, fx.project_id, seat, &listing_body(&refs, 25_000), 3).await?;
    let (server, client) = serve(Scripted::approving()).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "accepted", "{eval}");
    assert_eq!(eval["real_skills"]["seen"], REAL_SKILLS);
    assert_eq!(eval["real_skills"]["request_ids"], json!([carried]));
    assert_eq!(
        eval["real_skills"]["skills"].as_array().map(Vec::len),
        Some(SAMPLE_SIZE)
    );
    assert_eq!(
        eval["real_skills"]["listed"].as_array().map(Vec::len),
        Some(REAL_SKILLS),
        "the reviewer is shown every skill the sessions saw, not the sample"
    );
    let requests = server.received_requests().await.unwrap_or_default();
    let listings: Vec<String> = requests
        .iter()
        .filter_map(|r| serde_json::from_slice::<Value>(&r.body).ok())
        .filter(|body| message(body, 0).contains("followed by numbered messages"))
        .map(|body| message(&body, 1))
        .collect();
    assert_eq!(listings.len(), 2);
    for listing in &listings {
        let lines = listing.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(lines, 1 + SAMPLE_SIZE, "{listing}");
        assert!(listing.contains("- database-migrations: Ship a migration safely."));
    }
    fx.finish().await
}

#[tokio::test]
async fn a_draft_rejected_by_an_older_evaluator_is_evaluated_again() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert_evaluable(&fx).await?;
    sqlx::query(
        "UPDATE memory.skill_drafts
            SET status = 'rejected', evaluated_at = now(),
                eval = '{\"version\": 1, \"verdict\": {\"accepted\": false}}'::jsonb
          WHERE id = $1",
    )
    .bind(id)
    .execute(fx.handle.pool())
    .await?;
    let (_server, client) = serve(Scripted::approving()).await?;

    passes_until_decided(&fx, &client).await?;

    let (status, eval, _) = stored(&fx, id).await?;
    let eval = eval.ok_or_else(|| anyhow::anyhow!("an eval must be stored"))?;
    assert_eq!(status, "accepted", "{eval}");
    assert_eq!(eval["evaluator_version"], EVALUATOR_VERSION);
    assert!(eval.get("version").is_none());
    fx.finish().await
}
