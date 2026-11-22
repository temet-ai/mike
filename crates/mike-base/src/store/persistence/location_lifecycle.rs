use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutRow {
    pub location_uuid: Uuid,
    pub root_path: String,
    pub project_id: i64,
    pub project_uuid: Uuid,
    pub origin_url: Option<String>,
    pub parent_location_uuid: Option<Uuid>,
    pub present_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CheckoutMarks {
    pub locations: Vec<Uuid>,
    pub present: Vec<bool>,
    pub classified: Vec<Uuid>,
    pub main_roots: Vec<Option<String>>,
}

impl CheckoutMarks {
    pub fn presence(&mut self, location: Uuid, present: bool) {
        self.locations.push(location);
        self.present.push(present);
    }

    pub fn classify(&mut self, location: Uuid, main_root: Option<String>) {
        self.classified.push(location);
        self.main_roots.push(main_root);
    }
}

impl PersistHandle {
    pub async fn checkout_rows(&self, machine: Uuid) -> Result<Vec<CheckoutRow>, PersistError> {
        Ok(sqlx::query_as!(
            CheckoutRow,
            r#"SELECT l.location_uuid, l.root_path, r.id AS project_id, r.project_uuid,
                      l.origin_url, l.parent_location_uuid, l.present_at, l.created_at
                 FROM projects.locations l
                 JOIN projects.projects p ON p.id = l.project_id
                 JOIN projects.projects r ON r.id = COALESCE(p.merged_into_id, p.id)
                WHERE l.machine_uuid = $1
                ORDER BY l.root_path"#,
            machine,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn record_checkout_marks(
        &self,
        machine: Uuid,
        marks: &CheckoutMarks,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query!(
            "UPDATE projects.locations l
                SET gone_at = CASE WHEN m.present THEN NULL ELSE COALESCE(l.gone_at, now()) END,
                    present_at = CASE WHEN m.present THEN now() ELSE l.present_at END
               FROM unnest($2::uuid[], $3::bool[]) AS m(location_uuid, present)
              WHERE l.machine_uuid = $1 AND l.location_uuid = m.location_uuid
                AND CASE WHEN m.present
                         THEN l.gone_at IS NOT NULL OR l.present_at IS NULL
                              OR l.present_at < now() - interval '10 minutes'
                         ELSE l.gone_at IS NULL END",
            machine,
            &marks.locations[..],
            &marks.present[..],
        )
        .execute(&mut *tx)
        .await?;
        if marks.classified.is_empty() {
            tx.commit().await?;
            return Ok(());
        }
        sqlx::query!(
            "UPDATE projects.locations l SET parent_location_uuid = p.location_uuid
               FROM unnest($2::uuid[], $3::text[]) AS m(location_uuid, main_root)
               LEFT JOIN projects.locations p
                      ON p.machine_uuid = $1 AND rtrim(p.root_path, '/') = m.main_root
              WHERE l.machine_uuid = $1 AND l.location_uuid = m.location_uuid
                AND l.parent_location_uuid IS DISTINCT FROM p.location_uuid
                AND p.location_uuid IS DISTINCT FROM l.location_uuid",
            machine,
            &marks.classified[..],
            &marks.main_roots[..] as &[Option<String>],
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
