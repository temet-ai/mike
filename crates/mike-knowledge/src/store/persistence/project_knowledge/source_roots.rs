use uuid::Uuid;

use crate::store::persistence::PersistError;
use crate::store::persistence::location_move::{LocationMove, MovedLocation};

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn rewrite_moved_source_roots(
        self,
        moved: &[MovedLocation],
    ) -> Result<u64, PersistError> {
        if moved.is_empty() {
            return Ok(0);
        }
        let locations: Vec<Uuid> = moved.iter().map(|m| m.location_uuid).collect();
        let from: Vec<String> = moved.iter().map(|m| m.from.clone()).collect();
        let to: Vec<String> = moved.iter().map(|m| m.to.clone()).collect();
        Ok(sqlx::query!(
            r#"UPDATE memory.knowledge_sources s
                  SET root_path = CASE
                        WHEN rtrim(s.root_path, '/') = m.old_root
                             OR starts_with(s.root_path, m.old_root || '/')
                        THEN m.new_root || substr(rtrim(s.root_path, '/'), length(m.old_root) + 1)
                        ELSE left(s.root_path, length(s.root_path) - length(m.old_memory))
                             || m.new_memory
                      END
                 FROM (SELECT x.location_uuid, rtrim(x.old_root, '/') AS old_root, x.new_root,
                              '/projects/' || regexp_replace(x.old_root, '[^A-Za-z0-9]', '-', 'g')
                                  || '/memory' AS old_memory,
                              '/projects/' || regexp_replace(x.new_root, '[^A-Za-z0-9]', '-', 'g')
                                  || '/memory' AS new_memory
                         FROM unnest($1::uuid[], $2::text[], $3::text[])
                              AS x(location_uuid, old_root, new_root)) m
                WHERE s.location_uuid = m.location_uuid
                  AND (rtrim(s.root_path, '/') = m.old_root
                       OR starts_with(s.root_path, m.old_root || '/')
                       OR right(s.root_path, length(m.old_memory)) = m.old_memory)"#,
            &locations[..],
            &from[..],
            &to[..],
        )
        .execute(self.pool())
        .await?
        .rows_affected())
    }

    pub async fn finish_location_moves(
        self,
        moves: &mut [LocationMove],
    ) -> Result<(), PersistError> {
        for report in moves.iter_mut() {
            report.sources_rewritten = self.rewrite_moved_source_roots(&report.moved).await?;
        }
        Ok(())
    }
}
