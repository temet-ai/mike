use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::machine_merge::{Pairs, merge_collisions};
use super::project_identity::invalid;
use super::{PersistError, PersistHandle};

type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MovedLocation {
    pub location_uuid: Uuid,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocationMove {
    pub location_uuid: Uuid,
    pub machine_uuid: Uuid,
    pub from: String,
    pub to: String,
    pub moved: Vec<MovedLocation>,
    pub merged: Vec<Uuid>,
    pub sessions_repointed: u64,
    pub sources_rewritten: u64,
    pub sources_merged: u64,
    pub documents_merged: u64,
}

struct MovingHead {
    machine_uuid: Uuid,
    root: String,
}

struct Collision {
    moving: Uuid,
    new_root: String,
    existing: Option<Uuid>,
    same_project: Option<bool>,
}

impl PersistHandle {
    pub async fn location_at(
        &self,
        machine: Uuid,
        root: &str,
    ) -> Result<Option<Uuid>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT location_uuid FROM projects.locations
              WHERE machine_uuid = $1 AND rtrim(root_path, '/') = rtrim($2, '/')
              ORDER BY last_seen DESC LIMIT 1",
            machine,
            root,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn move_location(
        &self,
        location: Uuid,
        new_root: &str,
    ) -> Result<LocationMove, PersistError> {
        let to = absolute_root(new_root)?;
        let mut tx = self.pool().begin().await?;
        let head = lock_head(&mut tx, location).await?;
        check_paths(&head.root, &to)?;
        let collisions = plan_collisions(&mut tx, head.machine_uuid, &head.root, &to).await?;
        let pairs = merge_pairs(&collisions)?;
        let merged: Vec<Uuid> = collisions.iter().filter_map(|c| c.existing).collect();
        adopt_children(&mut tx, &collisions).await?;
        let counts = merge_collisions(&mut tx, &pairs).await?;
        let moved = rewrite_roots(&mut tx, head.machine_uuid, &head.root, &to).await?;
        let sources_rewritten = rewrite_source_roots(&mut tx, &moved).await?;
        tx.commit().await?;
        Ok(LocationMove {
            location_uuid: location,
            machine_uuid: head.machine_uuid,
            from: head.root,
            to,
            moved,
            merged,
            sessions_repointed: counts.sessions,
            sources_rewritten,
            sources_merged: counts.sources,
            documents_merged: counts.documents,
        })
    }
}

fn absolute_root(raw: &str) -> Result<String, PersistError> {
    let root = raw.trim().trim_end_matches('/');
    if !root.starts_with('/') {
        return Err(invalid(&format!(
            "The new root `{raw}` is not an absolute path; give the folder's full path"
        )));
    }
    Ok(root.to_owned())
}

fn check_paths(from: &str, to: &str) -> Result<(), PersistError> {
    if from == to {
        return Err(invalid(&format!("The location is already at `{to}`")));
    }
    if to.starts_with(&format!("{from}/")) || from.starts_with(&format!("{to}/")) {
        return Err(invalid(&format!(
            "Cannot move `{from}` to `{to}`: one folder contains the other"
        )));
    }
    Ok(())
}

fn merge_pairs(collisions: &[Collision]) -> Result<Pairs, PersistError> {
    if let Some(foreign) = collisions.iter().find(|c| c.same_project == Some(false)) {
        return Err(invalid(&format!(
            "`{}` is already a location of another project; merge the projects first or move it elsewhere",
            foreign.new_root
        )));
    }
    Ok(Pairs::from_rows(
        collisions
            .iter()
            .filter_map(|c| c.existing.map(|existing| (existing, c.moving)))
            .collect(),
    ))
}

async fn lock_head(tx: &mut Tx<'_>, location: Uuid) -> Result<MovingHead, PersistError> {
    sqlx::query_as!(
        MovingHead,
        r#"SELECT machine_uuid, rtrim(root_path, '/') AS "root!"
             FROM projects.locations WHERE location_uuid = $1 FOR UPDATE"#,
        location,
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid(&format!("Unknown location `{location}`")))
}

async fn plan_collisions(
    tx: &mut Tx<'_>,
    machine: Uuid,
    from: &str,
    to: &str,
) -> Result<Vec<Collision>, PersistError> {
    Ok(sqlx::query_as!(
        Collision,
        r#"WITH moving AS (
               SELECT l.location_uuid,
                      $3 || substr(rtrim(l.root_path, '/'), length($2) + 1) AS new_root,
                      COALESCE(p.merged_into_id, p.id) AS project_id
                 FROM projects.locations l
                 JOIN projects.projects p ON p.id = l.project_id
                WHERE l.machine_uuid = $1
                  AND (rtrim(l.root_path, '/') = $2 OR starts_with(l.root_path, $2 || '/'))
                  FOR UPDATE OF l
           )
           SELECT m.location_uuid AS "moving!", m.new_root AS "new_root!",
                  e.location_uuid AS "existing?",
                  COALESCE(ep.merged_into_id, ep.id) = m.project_id AS "same_project?"
             FROM moving m
             LEFT JOIN projects.locations e
                    ON e.machine_uuid = $1 AND rtrim(e.root_path, '/') = m.new_root
             LEFT JOIN projects.projects ep ON ep.id = e.project_id
            ORDER BY m.new_root"#,
        machine,
        from,
        to,
    )
    .fetch_all(&mut **tx)
    .await?)
}

async fn adopt_children(tx: &mut Tx<'_>, collisions: &[Collision]) -> Result<(), PersistError> {
    let (losers, winners): (Vec<Uuid>, Vec<Uuid>) = collisions
        .iter()
        .filter_map(|c| c.existing.map(|existing| (existing, c.moving)))
        .unzip();
    if losers.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE projects.locations l SET parent_location_uuid = p.winner
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
          WHERE l.parent_location_uuid = p.loser AND l.location_uuid <> p.winner",
        &losers[..],
        &winners[..],
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn rewrite_roots(
    tx: &mut Tx<'_>,
    machine: Uuid,
    from: &str,
    to: &str,
) -> Result<Vec<MovedLocation>, PersistError> {
    Ok(sqlx::query_as!(
        MovedLocation,
        r#"UPDATE projects.locations l
              SET root_path = $3 || substr(rtrim(l.root_path, '/'), length($2) + 1),
                  gone_at = NULL, last_seen = now()
            WHERE l.machine_uuid = $1
              AND (rtrim(l.root_path, '/') = $2 OR starts_with(l.root_path, $2 || '/'))
        RETURNING l.location_uuid AS "location_uuid!", old.root_path AS "from!",
                  new.root_path AS "to!""#,
        machine,
        from,
        to,
    )
    .fetch_all(&mut **tx)
    .await?)
}

async fn rewrite_source_roots(
    tx: &mut Tx<'_>,
    moved: &[MovedLocation],
) -> Result<u64, PersistError> {
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
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

#[cfg(test)]
#[path = "location_move_db_tests.rs"]
pub(crate) mod db_tests;
