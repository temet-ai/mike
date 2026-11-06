use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, sqlx::FromRow)]
pub struct ImprovementSummary {
    pub candidates_waiting: i64,
    pub directive_proposals_waiting: i64,
    pub guard_wakes_unlabelled: i64,
    pub guard_wakes_7d: i64,
    pub guard_appends_7d: i64,
    pub lessons_approved_7d: i64,
    pub lessons_rejected_7d: i64,
    pub proposals_decided_7d: i64,
    pub proposals_approved_7d: i64,
    pub proposals_rejected_7d: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct GuardCounts {
    unlabelled: i64,
    wakes_7d: i64,
    appends_7d: i64,
}

#[derive(Debug, Clone, Copy)]
struct BaseCounts {
    candidates_waiting: i64,
    directive_proposals_waiting: i64,
    lessons_approved_7d: i64,
    lessons_rejected_7d: i64,
    proposals_decided_7d: i64,
    proposals_approved_7d: i64,
    proposals_rejected_7d: i64,
}

impl PersistHandle {
    pub async fn improvement_counts(&self) -> Result<ImprovementSummary, PersistError> {
        crate::metrics::time_db("improvement_counts", async move {
            let base = sqlx::query_as!(
                BaseCounts,
                r#"WITH cand AS (
                       SELECT count(*) AS n FROM memory.lessons l
                         JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                         JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
                        WHERE ls.code = 'raw' AND lsrc.code = 'judge'
                   ),
                   waiting AS (
                       SELECT count(*) AS n FROM memory.directive_proposals WHERE status = 'open'
                   ),
                   decisions AS (
                       SELECT count(*) FILTER (WHERE cl.new_value ->> 'stage' = 'daily') AS approved,
                              count(*) FILTER (WHERE cl.new_value ->> 'stage' = 'archived') AS rejected
                         FROM config.change_log cl
                        WHERE cl.subject_kind = 'lesson_decision'
                          AND cl.changed_at >= NOW() - interval '7 days'
                   ),
                   decided AS (
                       SELECT count(*) AS decided,
                              count(*) FILTER (WHERE status = 'approved') AS approved,
                              count(*) FILTER (WHERE status = 'rejected') AS rejected
                         FROM memory.directive_proposals
                        WHERE status IN ('approved', 'rejected')
                          AND decided_at >= NOW() - interval '7 days'
                   )
                   SELECT cand.n AS "candidates_waiting!", waiting.n AS "directive_proposals_waiting!",
                          decisions.approved AS "lessons_approved_7d!",
                          decisions.rejected AS "lessons_rejected_7d!",
                          decided.decided AS "proposals_decided_7d!",
                          decided.approved AS "proposals_approved_7d!",
                          decided.rejected AS "proposals_rejected_7d!"
                     FROM cand, waiting, decisions, decided"#,
            )
            .fetch_one(&self.pool)
            .await?;
            let guard = self.guard_counts().await?;
            Ok(ImprovementSummary {
                candidates_waiting: base.candidates_waiting,
                directive_proposals_waiting: base.directive_proposals_waiting,
                guard_wakes_unlabelled: guard.unlabelled,
                guard_wakes_7d: guard.wakes_7d,
                guard_appends_7d: guard.appends_7d,
                lessons_approved_7d: base.lessons_approved_7d,
                lessons_rejected_7d: base.lessons_rejected_7d,
                proposals_decided_7d: base.proposals_decided_7d,
                proposals_approved_7d: base.proposals_approved_7d,
                proposals_rejected_7d: base.proposals_rejected_7d,
            })
        })
        .await
    }

    async fn guard_counts(&self) -> Result<GuardCounts, PersistError> {
        let Some(steerer) = self.guard_steerer().await? else {
            return Ok(GuardCounts::default());
        };
        let counts = sqlx::query!(
            r#"SELECT count(DISTINCT (e.operator_turn_id, e.question))
                        FILTER (WHERE e.outcome_label IS NULL
                                  AND rx.reaction IN ('accepted', 'redirected', 'corrected',
                                                      'closed_in_frustration', 'no_response',
                                                      'unclear')) AS "unlabelled!",
                      count(*) FILTER (WHERE i.delivery_channel = 'wake'
                                         AND e.dispatched_at >= NOW() - interval '7 days')
                          AS "wakes_7d!",
                      count(*) FILTER (WHERE i.delivery_channel = 'append'
                                         AND e.dispatched_at >= NOW() - interval '7 days')
                          AS "appends_7d!"
                 FROM steering.evaluations e
                 JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                 LEFT JOIN LATERAL (
                     SELECT v.reaction
                       FROM memory.turn_verdicts v
                      WHERE e.outcome_label IS NULL
                        AND v.operator_turn_id = e.operator_turn_id AND v.mode = 'live'
                      ORDER BY v.created_at DESC, v.id DESC
                      LIMIT 1
                 ) rx ON true
                WHERE e.steerer_id = $1
                  AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL)"#,
            steerer.id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(GuardCounts {
            unlabelled: counts.unlabelled,
            wakes_7d: counts.wakes_7d,
            appends_7d: counts.appends_7d,
        })
    }
}
