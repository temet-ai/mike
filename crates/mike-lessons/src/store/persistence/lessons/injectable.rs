use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

pub use crate::lesson_cache::marks::ChangeMark;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InjectableLessonRow {
    pub id: i64,
    pub project_path: String,
    pub kind: String,
    pub text: String,
    pub topic: Option<String>,
    pub agent_id: Option<i64>,
    pub revision: i32,
    pub route: Option<String>,
}

const FINGERPRINT: &str = "SELECT g.generation,
        (SELECT count(*) FROM memory.lessons l
          WHERE l.expires_at IS NOT NULL AND l.expires_at <= NOW()),
        (SELECT min(l.expires_at) FROM memory.lessons l WHERE l.expires_at > NOW())
   FROM memory.change_generations g
  WHERE g.name = 'lessons'";

struct InjectableLessons;

#[derive(Clone)]
struct Expiring {
    mark: ChangeMark,
    next_expiry: Option<DateTime<Utc>>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn injectable_lessons_fingerprint(self) -> Result<ChangeMark, PersistError> {
        let kept = self
            .kept_generation_while::<InjectableLessons, _, _, _>(
                || self.read_injectable_lessons_fingerprint(),
                |kept: &Expiring| kept.next_expiry.is_none_or(|at| at > Utc::now()),
            )
            .await?;
        Ok(kept.mark)
    }

    async fn read_injectable_lessons_fingerprint(self) -> Result<Expiring, PersistError> {
        crate::metrics::time_db("injectable_lessons_fingerprint", async move {
            let row: (i64, i64, Option<DateTime<Utc>>) =
                sqlx::query_as(FINGERPRINT).fetch_one(&self.pool).await?;
            Ok(Expiring {
                mark: ChangeMark {
                    generation: row.0,
                    expired: row.1,
                },
                next_expiry: row.2,
            })
        })
        .await
    }

    pub async fn list_project_injectable_lessons(
        self,
    ) -> Result<Vec<InjectableLessonRow>, PersistError> {
        let machine = crate::machine_identity::load_default()
            .await
            .ok()
            .map(|identity| identity.machine_uuid);
        crate::metrics::time_db("list_project_injectable_lessons", async move {
            let rows = sqlx::query_as::<_, InjectableLessonRow>( // grader-allow: reads memory.lessons.revision and memory.lesson_routes, added in 20261115140100, before their .sqlx entries are prepared
                r#"SELECT l.id, p.project_path, lk.code AS kind, l.text, l.topic, l.agent_id,
                          l.revision, lr.route
                     FROM memory.lessons l
                     JOIN projects.projects p ON p.id = l.project_id
                     LEFT JOIN memory.lesson_routes lr ON lr.lesson_id = l.id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                     JOIN memory.lesson_sources src ON src.id = l.source_id
                    WHERE (ls.code IN ('daily', 'long_term') OR (ls.code = 'raw' AND src.code = 'operator'))
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                      AND l.retired_at IS NULL
                      AND (l.machine_uuid IS NULL OR l.machine_uuid = $1)
                      AND NOT EXISTS (
                          SELECT 1
                            FROM memory.lesson_relations r
                            JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                            JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                           WHERE r.to_lesson_id = l.id
                      )
                      AND (ls.code <> 'raw' OR NOT EXISTS (
                          SELECT 1 FROM memory.lesson_shape_findings f
                           WHERE f.lesson_id = l.id AND f.resolution IS NULL
                      ))
                    ORDER BY l.created_at DESC"#,
            )
            .bind(machine)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
