use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::super::PersistError;
use crate::workspace::Liveness;

pub fn parse_liveness(s: &str) -> Liveness {
    Liveness::from_db(s)
}

#[derive(Debug, Clone)]
pub struct FleetNodeRow {
    pub session_id: i64,
    pub session_uuid: Uuid,
    pub title: Option<String>,
    pub last_status: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub request_count: i64,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub worktree: Option<String>,
    pub goal: Option<String>,
    pub next_steps: Option<String>,
    pub declared_role: Option<String>,
    pub agent_type: Option<String>,
    pub session_outcome: Option<String>,
    pub outcome_rationale: Option<String>,
    pub registration_liveness: Option<Liveness>,
    pub unconsumed_mail: i64,
}

struct ScopedSessionRow {
    id: i64,
    session_uuid: Uuid,
    title: Option<String>,
    last_status: Option<String>,
    last_seen: DateTime<Utc>,
}

#[derive(Default)]
struct NodeEnrichmentRow {
    session_id: i64,
    model: Option<String>,
    request_count: i64,
    cwd: Option<String>,
    worktree: Option<String>,
    goal: Option<String>,
    next_steps: Option<String>,
    declared_role: Option<String>,
    agent_type: Option<String>,
    session_outcome: Option<String>,
    outcome_rationale: Option<String>,
    liveness: Option<String>,
    unconsumed_mail: i64,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn fetch_fleet_nodes(
        self,
        project_id: i64,
        window_hours: i32,
    ) -> Result<Vec<FleetNodeRow>, PersistError> {
        crate::metrics::time_db("fleet_fetch_nodes", async move {
            let scoped = sqlx::query_as!(
                ScopedSessionRow,
                r#"SELECT s.id, s.session_uuid, s.title, s.last_status, s.last_seen
                     FROM sessions.sessions s
                    WHERE s.project_id = $1
                      AND (
                          s.last_seen >= now() - make_interval(hours => $2)
                          OR EXISTS (
                              SELECT 1 FROM workspace.session_registrations wr
                               WHERE wr.session_id = s.id
                                 AND wr.released_at IS NULL
                                 AND wr.lease_expires_at >= now()
                          )
                      )
                    ORDER BY s.last_seen DESC"#,
                project_id,
                window_hours,
            )
            .fetch_all(&self.pool)
            .await?;

            let session_ids: Vec<i64> = scoped.iter().map(|s| s.id).collect();
            let mut enrichment = self.fetch_fleet_node_enrichment(&session_ids).await?;

            Ok(scoped
                .into_iter()
                .map(|s| {
                    let e = enrichment.remove(&s.id).unwrap_or_default();
                    FleetNodeRow {
                        session_id: s.id,
                        session_uuid: s.session_uuid,
                        title: s.title,
                        last_status: s.last_status,
                        last_activity_at: s.last_seen,
                        request_count: e.request_count,
                        model: e.model,
                        cwd: e.cwd,
                        worktree: e.worktree,
                        goal: e.goal,
                        next_steps: e.next_steps,
                        declared_role: e.declared_role,
                        agent_type: e.agent_type,
                        session_outcome: e.session_outcome,
                        outcome_rationale: e.outcome_rationale,
                        registration_liveness: e.liveness.as_deref().map(parse_liveness),
                        unconsumed_mail: e.unconsumed_mail,
                    }
                })
                .collect())
        })
        .await
    }

    async fn fetch_fleet_node_enrichment(
        self,
        session_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, NodeEnrichmentRow>, PersistError> {
        let rows = sqlx::query_as!(
            NodeEnrichmentRow,
            r#"WITH ids AS (
                   SELECT unnest($1::bigint[]) AS session_id
               ),
               req_latest AS (
                   SELECT DISTINCT ON (r.session_id) r.session_id, r.model
                     FROM requests.requests r
                    WHERE r.session_id = ANY($1) AND r.model IS NOT NULL
                    ORDER BY r.session_id, r.created_at DESC
               ),
               req_counts AS (
                   SELECT r.session_id, COUNT(*) AS request_count,
                          MAX(r.created_at) AS last_request_at
                     FROM requests.requests r
                    WHERE r.session_id = ANY($1)
                    GROUP BY r.session_id
               ),
               roster AS (
                   SELECT DISTINCT ON (wr.session_id) wr.session_id, wr.cwd, wr.worktree,
                          wr.released_at, wr.lease_expires_at, wr.registered_at
                     FROM workspace.session_registrations wr
                    WHERE wr.session_id = ANY($1)
                    ORDER BY wr.session_id, wr.registered_at DESC
               ),
               goals AS (
                   SELECT sg.session_id, sg.goal, sg.next_steps, sg.role
                     FROM sessions.session_goals sg
                    WHERE sg.session_id = ANY($1)
               ),
               subagent AS (
                   SELECT DISTINCT ON (ai.session_id) ai.session_id, ag.name AS agent_type
                     FROM agents.instances ai
                     JOIN agents.agents ag ON ag.id = ai.agent_id
                    WHERE ai.session_id = ANY($1)
                    ORDER BY ai.session_id, ai.created_at DESC
               ),
               outcome AS (
                   SELECT DISTINCT ON (v.session_id) v.session_id, o.code AS outcome, v.rationale
                     FROM session_judges.verdicts v
                     JOIN session_judges.outcomes o ON o.id = v.outcome_id
                    WHERE v.session_id = ANY($1)
                      AND v.mode = 'live'
                    ORDER BY v.session_id, v.judged_at DESC, v.id DESC
               ),
               mail AS (
                   SELECT to_session_id AS session_id,
                          COUNT(*) FILTER (
                              WHERE consumed_at IS NULL
                                AND dead_lettered_at IS NULL
                                AND sent_at IS NOT NULL
                          ) AS unconsumed
                     FROM mailbox.messages
                    WHERE to_session_id = ANY($1)
                    GROUP BY to_session_id
               )
               SELECT
                   ids.session_id                 AS "session_id!",
                   rl.model                       AS "model?",
                   COALESCE(rc.request_count, 0)  AS "request_count!",
                   ro.cwd                         AS "cwd?",
                   ro.worktree                    AS "worktree?",
                   CASE
                       WHEN ro.session_id IS NULL THEN NULL
                       WHEN ro.released_at IS NOT NULL THEN 'ended'
                       WHEN rc.last_request_at IS NULL
                            AND ro.registered_at < now() - interval '10 minutes' THEN 'uncaptured'
                       WHEN ro.lease_expires_at > now()
                            OR rc.last_request_at > now() - interval '15 minutes' THEN 'live'
                       ELSE 'stale'
                   END                            AS "liveness?",
                   g.goal                         AS "goal?",
                   g.next_steps                   AS "next_steps?",
                   g.role                         AS "declared_role?",
                   sa.agent_type                  AS "agent_type?",
                   oc.outcome                     AS "session_outcome?",
                   oc.rationale                   AS "outcome_rationale?",
                   COALESCE(mail.unconsumed, 0)   AS "unconsumed_mail!"
               FROM ids
               LEFT JOIN req_latest rl ON rl.session_id = ids.session_id
               LEFT JOIN req_counts rc ON rc.session_id = ids.session_id
               LEFT JOIN roster ro ON ro.session_id = ids.session_id
               LEFT JOIN goals g ON g.session_id = ids.session_id
               LEFT JOIN subagent sa ON sa.session_id = ids.session_id
               LEFT JOIN outcome oc ON oc.session_id = ids.session_id
               LEFT JOIN mail ON mail.session_id = ids.session_id"#,
            session_ids,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.session_id, r)).collect())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parse_liveness_unknown_string_reads_as_stale() {
        assert!(matches!(parse_liveness("bogus"), Liveness::Stale));
    }
}
