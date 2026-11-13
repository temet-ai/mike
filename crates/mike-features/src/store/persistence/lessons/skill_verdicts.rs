use serde::Serialize;

use crate::store::persistence::PersistError;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerdictFollowThrough {
    pub placed_as_rule: u64,
    pub moved_to_skill: u64,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn apply_skill_place_verdicts(self) -> Result<VerdictFollowThrough, PersistError> {
        crate::metrics::time_db("apply_skill_place_verdicts", async move {
            let placed_as_rule = sqlx::query!(
                r#"WITH newest AS (
                       SELECT DISTINCT ON (d.project_id, d.folder)
                              d.id, d.project_id, d.topic, d.lesson_ids, d.review, d.eval
                         FROM memory.skill_drafts d
                        ORDER BY d.project_id, d.folder, d.id DESC),
                   verdicts AS (
                       SELECT n.id, n.project_id, n.topic, n.lesson_ids,
                              COALESCE(n.eval #>> '{review,reason}', '') AS reason
                         FROM newest n
                        WHERE n.review IS NULL
                          AND n.eval #>> '{review,decision}' = 'wrong_place'
                          AND n.eval #>> '{review,belongs_in}' = 'rule')
                   UPDATE memory.lessons l
                      SET placement_id = rule.id,
                          placement_reason = 'The skill reviewer of draft ' || v.id
                                             || ' placed this lesson in a rule: ' || v.reason,
                          placed_at = now()
                     FROM verdicts v
                     JOIN memory.lesson_placements rule ON rule.code = 'rule'
                     JOIN memory.lesson_placements skill ON skill.code = 'skill'
                    WHERE l.project_id = v.project_id
                      AND l.topic = v.topic
                      AND l.id = ANY (v.lesson_ids)
                      AND l.archived_at IS NULL
                      AND (l.placement_id IS NULL OR l.placement_id = skill.id)"#
            )
            .execute(&self.pool)
            .await?
            .rows_affected();
            let moved_to_skill = sqlx::query!(
                r#"WITH newest AS (
                       SELECT DISTINCT ON (d.project_id, d.folder)
                              d.project_id, d.folder, d.topic, d.agent_id, d.lesson_ids,
                              d.review, d.eval
                         FROM memory.skill_drafts d
                        ORDER BY d.project_id, d.folder, d.id DESC),
                   moves AS (
                       SELECT n.project_id, n.topic AS from_topic, n.lesson_ids,
                              t.topic AS to_topic, t.agent_id AS to_agent
                         FROM newest n
                         JOIN newest t ON t.project_id = n.project_id
                                      AND t.folder = n.eval #>> '{review,overlap}'
                        WHERE n.review IS NULL
                          AND n.eval #>> '{review,decision}' = 'wrong_place'
                          AND n.eval #>> '{review,belongs_in}' = 'existing_skill'
                          AND t.topic <> n.topic)
                   UPDATE memory.lessons l
                      SET topic = m.to_topic
                     FROM moves m
                    WHERE l.project_id = m.project_id
                      AND l.topic = m.from_topic
                      AND l.id = ANY (m.lesson_ids)
                      AND l.archived_at IS NULL
                      AND (m.to_agent IS NULL OR l.agent_id = m.to_agent)"#
            )
            .execute(&self.pool)
            .await?
            .rows_affected();
            Ok(VerdictFollowThrough {
                placed_as_rule,
                moved_to_skill,
            })
        })
        .await
    }
}
