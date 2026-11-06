use mike::store::persistence::lessons::skill_draft_eval::ShippedPatch;
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use serde_json::json;

use super::skill_draft_eval::insert;
use super::skill_drafts::{Fixture, draft, fixture};

#[tokio::test]
async fn an_accepted_patch_replaces_the_description_and_skill_md_and_a_rejection_never_does()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let kept = insert(&fx, "kept", 10, "original").await?;
    let patched = insert(&fx, "patched", 20, "original").await?;
    let rejected = insert(&fx, "rejected", 30, "original").await?;
    let eval = json!({"verdict": {"accepted": true}});
    let patch = ShippedPatch {
        description: "patched description",
        skill_md: "patched skill md",
    };
    for (id, accepted, shipped) in [
        (kept, true, None),
        (patched, true, Some(patch)),
        (rejected, false, None),
    ] {
        fx.handle.claim_skill_draft_for_eval().await?;
        assert!(
            fx.handle
                .finish_skill_draft_eval(id, accepted, &eval, shipped)
                .await?
        );
    }
    let row = |id: i64| {
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT status, description, skill_md FROM memory.skill_drafts WHERE id = $1",
        )
        .bind(id)
        .fetch_one(fx.handle.pool())
    };
    let kept_row = row(kept).await?;
    assert_eq!(
        (kept_row.0.as_str(), kept_row.1.as_str()),
        ("accepted", "original")
    );
    assert_ne!(kept_row.2, "patched skill md");
    assert_eq!(
        row(patched).await?,
        (
            "accepted".to_owned(),
            "patched description".to_owned(),
            "patched skill md".to_owned()
        )
    );
    assert_eq!(row(rejected).await?.1, "original");
    fx.finish().await
}

async fn reject_with_review(
    fx: &Fixture,
    target: &DraftTarget,
    ids: &[i64],
    decision: &str,
) -> anyhow::Result<()> {
    let empty = json!([]);
    let id = fx
        .handle
        .insert_skill_draft(draft(target, ids, &empty))
        .await?;
    fx.handle.claim_skill_draft_for_eval().await?;
    let eval = json!({
        "verdict": {"accepted": false, "reasons": ["the reviewer rejected it"]},
        "review": {"decision": decision, "reason": "x", "overlap": "deploy-and-restart"}
    });
    fx.handle
        .finish_skill_draft_eval(id, false, &eval, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn content_the_reviewer_placed_elsewhere_is_not_redrafted_until_a_new_lesson_arrives()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx.daily_lesson("Run migrate-check first.").await?;
    let b = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;
    for decision in ["wrong_place", "nothing_to_save"] {
        let target = fx
            .handle
            .next_skill_draft_target()
            .await?
            .ok_or_else(|| anyhow::anyhow!("{decision}: the topic must be offered first"))?;
        reject_with_review(&fx, &target, &[a, b], decision).await?;
        assert!(
            fx.handle.next_skill_draft_target().await?.is_none(),
            "{decision} must stop redrafting at this watermark"
        );
        sqlx::query("DELETE FROM memory.skill_drafts")
            .execute(fx.handle.pool())
            .await?;
    }
    let target = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("the topic must be offered again"))?;
    reject_with_review(&fx, &target, &[a, b], "patch").await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_some(),
        "an ordinary rejection is still redrafted"
    );
    reject_with_review(&fx, &target, &[a, b], "wrong_place").await?;
    fx.daily_lesson("Number a migration above the applied one.")
        .await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_some(),
        "a new lesson reopens a topic the reviewer placed elsewhere"
    );
    fx.finish().await
}

#[tokio::test]
async fn the_agent_a_draft_is_scoped_to_is_read_with_its_description_and_definition()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let agent: i64 = sqlx::query_scalar(
        "INSERT INTO agents.agents (name, description, definition_path)
         VALUES ('fleet-coordinator', 'Coordinates a fleet.', '.claude/agents/fleet-coordinator.md')
         RETURNING id",
    )
    .fetch_one(fx.handle.pool())
    .await?;
    let found = fx
        .handle
        .skill_draft_agent_scope(agent)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the agent must be found"))?;
    assert_eq!(found.name, "fleet-coordinator");
    assert_eq!(found.description.as_deref(), Some("Coordinates a fleet."));
    assert_eq!(
        found.definition_path.as_deref(),
        Some(".claude/agents/fleet-coordinator.md")
    );
    assert!(
        fx.handle
            .skill_draft_agent_scope(agent + 1000)
            .await?
            .is_none()
    );
    fx.finish().await
}

#[tokio::test]
async fn lessons_are_read_by_id_with_their_kind_and_source_prompt() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx.daily_lesson("Run migrate-check first.").await?;
    let b = fx.daily_lesson("Commit with the code.").await?;
    let found = fx.handle.skill_draft_lessons_by_id(&[b, a]).await?;
    let texts: Vec<&str> = found.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["Run migrate-check first.", "Commit with the code."]
    );
    assert_eq!(found[0].kind, "convention");
    assert!(fx.handle.skill_draft_lessons_by_id(&[]).await?.is_empty());
    fx.finish().await
}
