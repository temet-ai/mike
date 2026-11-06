use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use mike::store::persistence::lessons::skill_verdicts::VerdictFollowThrough;
use serde_json::json;

use super::skill_drafts::{Fixture, draft, fixture};

impl Fixture {
    async fn skill_lesson(&self, topic: &str, text: &str) -> anyhow::Result<i64> {
        let row = self
            .handle
            .create_lesson_with_topic(NewLessonWrite {
                project_id: self.project_id,
                kind: "convention",
                text,
                topic: Some(topic),
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?;
        sqlx::query(
            "UPDATE memory.lessons
                SET stage_id = (SELECT id FROM memory.lesson_stages WHERE code = 'long_term'),
                    placement_id = (SELECT id FROM memory.lesson_placements WHERE code = 'skill'),
                    placed_at = now()
              WHERE id = $1",
        )
        .bind(row.id)
        .execute(self.handle.pool())
        .await?;
        Ok(row.id)
    }

    fn target(&self, topic: &str, watermark: i64) -> DraftTarget {
        DraftTarget {
            project_id: self.project_id,
            project_path: "/tmp/skill-drafts".to_owned(),
            folder: topic.to_owned(),
            topic: topic.to_owned(),
            agent_id: None,
            agent_name: None,
            lesson_watermark: watermark,
        }
    }

    async fn drafted(&self, topic: &str, ids: &[i64]) -> anyhow::Result<i64> {
        let watermark = ids.iter().copied().max().unwrap_or_default();
        let target = self.target(topic, watermark);
        let empty = json!([]);
        Ok(self
            .handle
            .insert_skill_draft(draft(&target, ids, &empty))
            .await?)
    }

    async fn reviewed(&self, draft_id: i64, review: serde_json::Value) -> anyhow::Result<()> {
        sqlx::query("UPDATE memory.skill_drafts SET status = 'rejected', eval = $2 WHERE id = $1")
            .bind(draft_id)
            .bind(json!({ "review": review }))
            .execute(self.handle.pool())
            .await?;
        Ok(())
    }

    async fn lesson_place(&self, id: i64) -> anyhow::Result<(String, String)> {
        Ok(sqlx::query_as(
            "SELECT l.topic, p.code FROM memory.lessons l
               JOIN memory.lesson_placements p ON p.id = l.placement_id
              WHERE l.id = $1",
        )
        .bind(id)
        .fetch_one(self.handle.pool())
        .await?)
    }
}

fn rule_verdict() -> serde_json::Value {
    json!({"decision": "wrong_place", "belongs_in": "rule", "reason": "A standing constraint."})
}

fn skill_verdict(overlap: &str) -> serde_json::Value {
    json!({"decision": "wrong_place", "belongs_in": "existing_skill", "overlap": overlap,
           "reason": "The existing skill already covers this."})
}

#[tokio::test]
async fn rule_verdict_places_only_the_drafted_lessons_as_rules_once() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx
        .skill_lesson("release-tagging", "Tag only from main.")
        .await?;
    let b = fx
        .skill_lesson("release-tagging", "Never move a tag.")
        .await?;
    let draft_id = fx.drafted("release-tagging", &[a, b]).await?;
    fx.reviewed(draft_id, rule_verdict()).await?;
    let later = fx
        .skill_lesson("release-tagging", "Sign release tags.")
        .await?;

    let done = fx.handle.apply_skill_place_verdicts().await?;
    assert_eq!(done.placed_as_rule, 2);
    assert_eq!(fx.lesson_place(a).await?.1, "rule");
    assert_eq!(fx.lesson_place(b).await?.1, "rule");
    assert_eq!(
        fx.lesson_place(later).await?.1,
        "skill",
        "a lesson the reviewed draft never saw must keep its placement"
    );
    assert_eq!(
        fx.handle.apply_skill_place_verdicts().await?,
        VerdictFollowThrough::default(),
        "a verdict already applied must change nothing"
    );
    fx.finish().await
}

#[tokio::test]
async fn existing_skill_verdict_moves_older_lessons_and_reopens_the_named_skill()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let s1 = fx
        .skill_lesson("fleet-task-verification", "Verify at the seat SHA.")
        .await?;
    let s2 = fx
        .skill_lesson("fleet-task-verification", "Name the failing check.")
        .await?;
    let t1 = fx
        .skill_lesson("fleet-coordination", "Answer blocked seats first.")
        .await?;
    let t2 = fx
        .skill_lesson("fleet-coordination", "Close the fleet on acceptance.")
        .await?;
    fx.drafted("fleet-coordination", &[t1, t2]).await?;
    let source = fx.drafted("fleet-task-verification", &[s1, s2]).await?;
    fx.reviewed(source, skill_verdict("fleet-coordination"))
        .await?;
    assert!(
        fx.handle.next_skill_draft_target().await?.is_none(),
        "both topics are drafted and held before the verdict is applied"
    );

    let done = fx.handle.apply_skill_place_verdicts().await?;
    assert_eq!(done.moved_to_skill, 2);
    assert_eq!(fx.lesson_place(s1).await?.0, "fleet-coordination");

    let reopened =
        fx.handle.next_skill_draft_target().await?.ok_or_else(|| {
            anyhow::anyhow!("the named skill gained lessons and must be redrafted")
        })?;
    assert_eq!(reopened.folder, "fleet-coordination");
    assert_eq!(reopened.lesson_watermark, t2);
    let ids: Vec<i64> = fx
        .handle
        .skill_draft_lessons(&reopened)
        .await?
        .iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, vec![s1, s2, t1, t2]);
    let empty = json!([]);
    fx.handle
        .insert_skill_draft(draft(&reopened, &ids, &empty))
        .await?;
    assert!(fx.handle.next_skill_draft_target().await?.is_none());
    fx.finish().await
}

#[tokio::test]
async fn verdict_with_an_operator_decision_is_left_alone() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx
        .skill_lesson("settings-ui", "Group settings by job.")
        .await?;
    let b = fx
        .skill_lesson("settings-ui", "Say when a change applies.")
        .await?;
    let t1 = fx
        .skill_lesson("ui-design", "Use the guide's tokens.")
        .await?;
    fx.drafted("ui-design", &[t1]).await?;
    let source = fx.drafted("settings-ui", &[a, b]).await?;
    fx.reviewed(source, skill_verdict("ui-design")).await?;
    sqlx::query("UPDATE memory.skill_drafts SET review = 'approved' WHERE id = $1")
        .bind(source)
        .execute(fx.handle.pool())
        .await?;

    assert_eq!(
        fx.handle.apply_skill_place_verdicts().await?,
        VerdictFollowThrough::default()
    );
    assert_eq!(fx.lesson_place(a).await?.0, "settings-ui");
    fx.finish().await
}

#[tokio::test]
async fn apply_skill_place_verdicts_on_a_closed_pool_is_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    fx.handle.pool().close().await;
    assert!(fx.handle.apply_skill_place_verdicts().await.is_err());
    fx.db.cleanup().await
}
