use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::ledger::{Live, LiveRow, Measured, Shipped};
use super::model::{DraftRow, LessonRow, skill_key};
use super::routes::RouteDecision;
use crate::lesson_worker::LessonRoute;

const CAP_SETTING: &str = "lessons.skills.max_per_project";
const LOAD_WINDOW_DAYS: i32 = 30;

pub(super) async fn accepted_lessons(pool: &PgPool, machine: Uuid) -> Result<Vec<LessonRow>> {
    sqlx::query_file_as!(
        LessonRow,
        "src/lesson_promotion/accepted_lessons.sql",
        machine
    )
    .fetch_all(pool)
    .await
    .context("reading accepted lessons")
}

pub(super) async fn drafts(pool: &PgPool) -> Result<Vec<DraftRow>> {
    sqlx::query_file_as!(DraftRow, "src/lesson_promotion/drafts.sql")
        .fetch_all(pool)
        .await
        .context("reading skill drafts")
}

pub(super) async fn policies(pool: &PgPool) -> Result<BTreeMap<String, String>> {
    let rows = sqlx::query!(
        "SELECT p.project_uuid, sp.mode
           FROM memory.skill_policies sp JOIN projects.projects p ON p.id = sp.project_id"
    )
    .fetch_all(pool)
    .await
    .context("reading skill policies")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.project_uuid.to_string(), row.mode))
        .collect())
}

pub(super) async fn user_retired(pool: &PgPool) -> Result<BTreeSet<String>> {
    let rows = sqlx::query!(
        "SELECT p.project_uuid, r.folder
           FROM memory.skill_retirements r JOIN projects.projects p ON p.id = r.project_id"
    )
    .fetch_all(pool)
    .await
    .context("reading retired skill folders")?;
    Ok(rows
        .into_iter()
        .map(|row| skill_key(&row.project_uuid, &row.folder))
        .collect())
}

pub(super) async fn locations(
    pool: &PgPool,
    machine: Uuid,
) -> Result<BTreeMap<String, Vec<String>>> {
    let rows = sqlx::query!(
        "SELECT p.project_uuid, l.root_path
           FROM projects.locations l JOIN projects.projects p ON p.id = l.project_id
          WHERE l.machine_uuid = $1
          ORDER BY l.last_seen DESC",
        machine
    )
    .fetch_all(pool)
    .await
    .context("reading this machine's project locations")?;
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        found
            .entry(row.project_uuid.to_string())
            .or_default()
            .push(row.root_path);
    }
    Ok(found)
}

pub(super) async fn skill_loads(pool: &PgPool) -> Result<BTreeMap<String, i64>> {
    let rows = sqlx::query!(
        r#"SELECT p.project_uuid AS "project_uuid?", u.input->>'skill' AS skill, count(*) AS "loads!"
             FROM tools.uses u
             JOIN requests.requests r ON r.id = u.request_id
             LEFT JOIN sessions.sessions s ON s.id = r.session_id
             LEFT JOIN projects.projects p ON p.id = coalesce(r.project_id, s.project_id)
            WHERE u.tool_name = 'Skill' AND u.created_at > now() - make_interval(days => $1)
            GROUP BY 1, 2"#,
        LOAD_WINDOW_DAYS
    )
    .fetch_all(pool)
    .await
    .context("counting skill loads")?;
    Ok(rows
        .into_iter()
        .filter_map(|row| Some((skill_key(&row.project_uuid?, &row.skill?), row.loads)))
        .collect())
}

pub(super) async fn skill_cap_setting(pool: &PgPool) -> Result<Option<serde_json::Value>> {
    let row = sqlx::query_scalar!(
        "SELECT value FROM config.settings WHERE key = $1",
        CAP_SETTING
    )
    .fetch_optional(pool)
    .await
    .context("reading the skill cap setting")?;
    Ok(row)
}

pub(super) async fn measure(pool: &PgPool, shipped: &[Shipped]) -> Result<Measured> {
    if shipped.is_empty() {
        return Ok(Measured::new());
    }
    let projects: Vec<Uuid> = shipped.iter().map(|skill| skill.project).collect();
    let folders: Vec<String> = shipped.iter().map(|skill| skill.folder.clone()).collect();
    let days: Vec<NaiveDate> = shipped.iter().map(|skill| skill.since).collect();
    let rows = sqlx::query_file_as!(
        LiveRow,
        "src/lesson_promotion/live.sql",
        &projects,
        &folders,
        &days,
        LOAD_WINDOW_DAYS
    )
    .fetch_all(pool)
    .await
    .context("measuring shipped skills")?;
    let since: BTreeMap<(Uuid, &str), NaiveDate> = shipped
        .iter()
        .map(|skill| ((skill.project, skill.folder.as_str()), skill.since))
        .collect();
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let first_shipped_on = *since.get(&(row.project_uuid, row.folder.as_str()))?;
            Some((
                skill_key(&row.project_uuid, &row.folder),
                Live {
                    row,
                    first_shipped_on,
                },
            ))
        })
        .collect())
}

pub(super) async fn existing_routes(pool: &PgPool) -> Result<BTreeMap<i64, LessonRoute>> {
    let rows = sqlx::query!("SELECT lesson_id, route FROM memory.lesson_routes")
        .fetch_all(pool)
        .await
        .context("reading lesson routes")?;
    Ok(rows
        .into_iter()
        .map(|row| (row.lesson_id, LessonRoute::from_code(Some(&row.route))))
        .collect())
}

pub(super) async fn save_routes(
    pool: &PgPool,
    decisions: &[RouteDecision],
    commit: bool,
) -> Result<u64> {
    if decisions.is_empty() {
        return Ok(0);
    }
    let lessons: Vec<i64> = decisions.iter().map(|d| d.lesson_id).collect();
    let projects: Vec<i64> = decisions.iter().map(|d| d.project_id).collect();
    let routes: Vec<String> = decisions
        .iter()
        .map(|d| d.route.code().to_owned())
        .collect();
    let covered: Vec<Option<String>> = decisions.iter().map(|d| d.covered_by.clone()).collect();
    let revisions: Vec<i32> = decisions.iter().map(|d| d.revision).collect();
    let mut tx = pool.begin().await?;
    let done = sqlx::query!(
        "INSERT INTO memory.lesson_routes AS r (lesson_id, project_id, route, covered_by, revision)
         SELECT d.lesson_id, d.project_id, d.route, d.covered_by, d.revision
           FROM unnest($1::bigint[], $2::bigint[], $3::text[], $4::text[], $5::int[])
                AS d(lesson_id, project_id, route, covered_by, revision)
           JOIN memory.lessons l ON l.id = d.lesson_id
         ON CONFLICT (lesson_id) DO UPDATE
            SET project_id = EXCLUDED.project_id, route = EXCLUDED.route,
                covered_by = EXCLUDED.covered_by, revision = EXCLUDED.revision,
                decided_at = now()
          WHERE (r.project_id, r.route, r.covered_by, r.revision)
                IS DISTINCT FROM (EXCLUDED.project_id, EXCLUDED.route,
                                  EXCLUDED.covered_by, EXCLUDED.revision)",
        &lessons,
        &projects,
        &routes,
        &covered as &[Option<String>],
        &revisions
    )
    .execute(&mut *tx)
    .await
    .context("saving lesson routes")?;
    if commit {
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }
    Ok(done.rows_affected())
}
