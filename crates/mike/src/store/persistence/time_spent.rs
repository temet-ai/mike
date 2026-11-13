use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TimeSpentFilter<'a> {
    pub(crate) project: Option<&'a str>,
    pub(crate) project_path: Option<&'a str>,
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
    pub(crate) include_subagents: bool,
    pub(crate) session: Option<Uuid>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(crate) struct TimeSpentByKindRow {
    pub(crate) kind: String,
    pub(crate) answers: i64,
    pub(crate) answers_over_an_hour: i64,
    pub(crate) wait_seconds: f64,
    pub(crate) p50_wait_seconds: f64,
    pub(crate) p90_wait_seconds: f64,
    pub(crate) model_seconds: f64,
    pub(crate) sessions: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(crate) struct SlowCommandRow {
    pub(crate) command: String,
    pub(crate) kind: String,
    pub(crate) calls: i64,
    pub(crate) seconds: f64,
    pub(crate) p50_seconds: f64,
    pub(crate) sessions: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(crate) struct BuildsRunningAtOnceRow {
    pub(crate) running_at_once: String,
    pub(crate) sort_key: i64,
    pub(crate) calls: i64,
    pub(crate) seconds: f64,
    pub(crate) p50_seconds: f64,
}

const CALLS_HEAD: &str = r#"WITH calls AS (
    SELECT u.id, u.request_id, u.session_id, u.tool_name,
           u.input ->> 'command' AS command,
           q.created_at AS answer_started_at,
           q.completed_at AS answer_completed_at,
           min(r.created_at) AS result_at
      FROM tools.uses u
      JOIN requests.requests q ON q.id = u.request_id
      JOIN projects.projects p ON p.id = q.project_id
      JOIN sessions.sessions s ON s.id = u.session_id
      JOIN tools.results r ON r.tool_use_id = u.id
     WHERE u.created_at >= "#;

const KIND_NAMES: &str = r#"(ARRAY['subagent', 'gate', 'rust tests', 'frontend tests', 'lint', 'rust build',
                  'deploy', 'waiting on background', 'database query', 'git', 'other shell',
                  'mcp tool', 'unused', 'read, edit, search'])"#;

const KINDED_CTE: &str = r#"
     GROUP BY u.id, q.created_at, q.completed_at
), kinded AS (
    SELECT c.id, c.request_id, c.session_id, c.tool_name, c.command,
           c.answer_started_at, c.answer_completed_at, c.result_at,
           extract(epoch FROM c.result_at - c.answer_completed_at)::float8 AS seconds,
           CASE
             WHEN c.tool_name IN ('Agent', 'Task') THEN 1
             WHEN c.tool_name <> 'Bash' THEN CASE WHEN c.tool_name LIKE 'mcp\_\_%' THEN 12 ELSE 14 END
             WHEN c.command ~ '(scripts/gate|make (check|gate))' THEN 2
             WHEN c.command ~ '(nextest|cargo test|cx test|make test)' THEN 3
             WHEN c.command ~ '(sb-test|vitest|playwright)' THEN 4
             WHEN c.command ~ '(clippy|make lint|lint-rust)' THEN 5
             WHEN c.command ~ '((cargo|cx) (build|check|rustc)|make (build|install))' THEN 6
             WHEN c.command ~ '(start-proxy|deploy-proxy)' THEN 7
             WHEN c.command ~ '(task-wait|sleep |until )' THEN 8
             WHEN c.command ~ '(psql|docker exec)' THEN 9
             WHEN c.command ~ '^\s*git ' THEN 10
             ELSE 11 END AS kind_rank
      FROM calls c
)"#;

const BY_KIND_TAIL: &str = r#"
, answers AS (
    SELECT request_id, session_id,
           min(kind_rank) AS kind_rank,
           extract(epoch FROM min(answer_completed_at) - min(answer_started_at))::float8 AS model_seconds,
           max(extract(epoch FROM result_at - answer_completed_at))::float8 AS wait_seconds
      FROM kinded
     GROUP BY request_id, session_id
), named AS (
    SELECT a.request_id, a.session_id, a.model_seconds, a.wait_seconds,
           KIND_NAMES[a.kind_rank] AS kind
      FROM answers a
)
SELECT kind AS kind,
       count(*) FILTER (WHERE wait_seconds BETWEEN 0 AND 3600)::bigint AS answers,
       count(*) FILTER (WHERE wait_seconds > 3600)::bigint AS answers_over_an_hour,
       coalesce(sum(wait_seconds) FILTER (WHERE wait_seconds BETWEEN 0 AND 3600), 0)::float8 AS wait_seconds,
       coalesce(percentile_cont(0.5) WITHIN GROUP (ORDER BY wait_seconds) FILTER (WHERE wait_seconds BETWEEN 0 AND 3600), 0)::float8 AS p50_wait_seconds,
       coalesce(percentile_cont(0.9) WITHIN GROUP (ORDER BY wait_seconds) FILTER (WHERE wait_seconds BETWEEN 0 AND 3600), 0)::float8 AS p90_wait_seconds,
       coalesce(sum(model_seconds), 0)::float8 AS model_seconds,
       count(DISTINCT session_id)::bigint AS sessions
  FROM named
 GROUP BY kind
 ORDER BY 4 DESC"#;

const SLOW_COMMANDS_HEAD: &str = r#"
, cmds AS (
    SELECT regexp_replace(regexp_replace(regexp_replace(regexp_replace(command,
                '/tmp/claude-[0-9]+/[^ ;|&"'']*', '<tmp>', 'g'),
                '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', '<id>', 'g'),
                '^\s*cd [^&;]+(&&|;)\s*', ''),
                '\s+', ' ', 'g') AS command,
           KIND_NAMES[kind_rank] AS kind,
           extract(epoch FROM result_at - answer_completed_at)::float8 AS seconds,
           session_id
      FROM kinded
     WHERE tool_name = 'Bash' AND command IS NOT NULL
       AND result_at - answer_completed_at BETWEEN interval '0' AND interval '1 hour'
)
SELECT command AS command, kind AS kind,
       count(*)::bigint AS calls,
       sum(seconds)::float8 AS seconds,
       percentile_cont(0.5) WITHIN GROUP (ORDER BY seconds)::float8 AS p50_seconds,
       count(DISTINCT session_id)::bigint AS sessions
  FROM cmds"#;

const SLOW_COMMANDS_TAIL: &str = r#"
 GROUP BY command, kind
 ORDER BY 4 DESC, 1, 2"#;

const BUILDS_HEAD: &str = r#"WITH builds AS (
    SELECT u.id, u.session_id, p.project_path,
           u.created_at AS started_at, min(r.created_at) AS ended_at,
           (q.agent_instance_id IS NULL AND q.subagent_persona IS NULL) AS main_session,
           s.session_uuid
      FROM tools.uses u
      JOIN requests.requests q ON q.id = u.request_id
      JOIN projects.projects p ON p.id = q.project_id
      JOIN sessions.sessions s ON s.id = u.session_id
      JOIN tools.results r ON r.tool_use_id = u.id
     WHERE u.tool_name = 'Bash'
       AND u.created_at >= "#;

const BUILDS_MIDDLE: &str = r#"
       AND (u.input ->> 'command') ~ '(nextest|(cargo|cx) (build|check|clippy|test|rustc)|make (test|lint|check|build|install)|scripts/gate)'
       AND coalesce((u.input ->> 'run_in_background')::bool, false) = false
     GROUP BY u.id, p.project_path, q.agent_instance_id, q.subagent_persona, s.session_uuid
    HAVING min(r.created_at) - u.created_at <= interval '1 hour'
), mine AS (
    SELECT b.id, b.session_id, b.started_at, b.ended_at
      FROM builds b
     WHERE b.started_at >= "#;

const BUILDS_TAIL: &str = r#"
), overlapped AS (
    SELECT m.id, extract(epoch FROM m.ended_at - m.started_at)::float8 AS seconds,
           count(o.id) AS others
      FROM mine m
      LEFT JOIN builds o
        ON o.session_id <> m.session_id AND o.started_at < m.ended_at AND o.ended_at > m.started_at
     GROUP BY m.id, m.started_at, m.ended_at
)
SELECT CASE WHEN others = 0 THEN 'none' WHEN others <= 2 THEN '1 to 2'
            WHEN others <= 5 THEN '3 to 5' ELSE '6 or more' END AS running_at_once,
       min(others)::bigint AS sort_key,
       count(*)::bigint AS calls,
       sum(seconds)::float8 AS seconds,
       percentile_cont(0.5) WITHIN GROUP (ORDER BY seconds)::float8 AS p50_seconds
  FROM overlapped
 GROUP BY 1
 ORDER BY 2"#;

fn calls_query(filter: &TimeSpentFilter<'_>) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::<Postgres>::new(CALLS_HEAD);
    qb.push_bind(filter.from)
        .push(" AND u.created_at < ")
        .push_bind(filter.to)
        .push(" AND q.completed_at IS NOT NULL");
    if let Some(project) = filter.project {
        qb.push(" AND p.project_path ILIKE '%' || ")
            .push_bind(project.to_owned())
            .push(" || '%'");
    }
    if let Some(project_path) = filter.project_path {
        qb.push(" AND p.project_path = ")
            .push_bind(project_path.to_owned());
    }
    if !filter.include_subagents {
        qb.push(" AND (q.agent_instance_id IS NULL AND q.subagent_persona IS NULL)");
    }
    if let Some(session) = filter.session {
        qb.push(" AND s.session_uuid = ").push_bind(session);
    }
    qb.push(KINDED_CTE);
    qb
}

fn with_kind_names(sql: &str) -> String {
    sql.replace("KIND_NAMES", KIND_NAMES)
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn time_spent_by_kind(
        self,
        filter: &TimeSpentFilter<'_>,
    ) -> Result<Vec<TimeSpentByKindRow>, PersistError> {
        crate::metrics::time_db("time_spent_by_kind", async move {
            let mut qb = calls_query(filter);
            qb.push(with_kind_names(BY_KIND_TAIL));
            let rows = qb
                .build_query_as::<TimeSpentByKindRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn time_spent_slow_commands(
        self,
        filter: &TimeSpentFilter<'_>,
        limit: i64,
        offset: i64,
        kind: Option<&str>,
    ) -> Result<Vec<SlowCommandRow>, PersistError> {
        crate::metrics::time_db("time_spent_slow_commands", async move {
            let mut qb = calls_query(filter);
            qb.push(with_kind_names(SLOW_COMMANDS_HEAD));
            if let Some(kind) = kind {
                qb.push(" WHERE kind = ").push_bind(kind.to_owned());
            }
            qb.push(SLOW_COMMANDS_TAIL)
                .push(" LIMIT ")
                .push_bind(limit)
                .push(" OFFSET ")
                .push_bind(offset);
            let rows = qb
                .build_query_as::<SlowCommandRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn time_spent_builds_running_at_once(
        self,
        filter: &TimeSpentFilter<'_>,
    ) -> Result<Vec<BuildsRunningAtOnceRow>, PersistError> {
        crate::metrics::time_db("time_spent_builds_running_at_once", async move {
            let mut qb = QueryBuilder::<Postgres>::new(BUILDS_HEAD);
            qb.push_bind(filter.from)
                .push("::timestamptz - interval '1 hour' AND u.created_at < ")
                .push_bind(filter.to)
                .push(BUILDS_MIDDLE)
                .push_bind(filter.from);
            if let Some(project) = filter.project {
                qb.push(" AND b.project_path ILIKE '%' || ")
                    .push_bind(project.to_owned())
                    .push(" || '%'");
            }
            if let Some(project_path) = filter.project_path {
                qb.push(" AND b.project_path = ")
                    .push_bind(project_path.to_owned());
            }
            if !filter.include_subagents {
                qb.push(" AND b.main_session");
            }
            if let Some(session) = filter.session {
                qb.push(" AND b.session_uuid = ").push_bind(session);
            }
            qb.push(BUILDS_TAIL);
            let rows = qb
                .build_query_as::<BuildsRunningAtOnceRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
