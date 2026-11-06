use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{PersistError, PersistHandle};

type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MachineMergeReport {
    pub(crate) locations_moved: u64,
    pub(crate) locations_merged: u64,
    pub(crate) sessions_repointed: u64,
    pub(crate) sources_merged: u64,
    pub(crate) documents_merged: u64,
    pub(crate) llm_calls_updated: u64,
    pub(crate) worker_leases_updated: u64,
}

#[derive(Debug, Default)]
struct Pairs {
    losers: Vec<Uuid>,
    winners: Vec<Uuid>,
}

impl Pairs {
    fn from_rows(rows: Vec<(Uuid, Uuid)>) -> Self {
        let (losers, winners) = rows.into_iter().unzip();
        Self { losers, winners }
    }

    fn len(&self) -> u64 {
        u64::try_from(self.losers.len()).unwrap_or(u64::MAX)
    }

    fn is_empty(&self) -> bool {
        self.losers.is_empty()
    }
}

#[derive(Debug, Default)]
struct CollisionCounts {
    sessions: u64,
    sources: u64,
    documents: u64,
}

impl PersistHandle {
    pub(crate) async fn merge_machine_identity(
        &self,
        legacy: Uuid,
        current: Uuid,
        current_name: &str,
    ) -> Result<MachineMergeReport, PersistError> {
        let mut tx = self.pool().begin().await?;
        ensure_machine(&mut tx, current, current_name).await?;
        let pairs = colliding_locations(&mut tx, legacy, current).await?;
        let collisions = merge_collisions(&mut tx, &pairs).await?;
        let locations_moved = move_locations(&mut tx, legacy, current).await?;
        sqlx::query!(
            "DELETE FROM projects.machines WHERE machine_uuid = $1",
            legacy
        )
        .execute(&mut *tx)
        .await?;
        let llm_calls_updated = retag_llm_calls(&mut tx, legacy, current).await?;
        let worker_leases_updated = retag_worker_leases(&mut tx, legacy, current).await?;
        tx.commit().await?;
        Ok(MachineMergeReport {
            locations_moved,
            locations_merged: pairs.len(),
            sessions_repointed: collisions.sessions,
            sources_merged: collisions.sources,
            documents_merged: collisions.documents,
            llm_calls_updated,
            worker_leases_updated,
        })
    }
}

async fn ensure_machine(tx: &mut Tx<'_>, machine: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO projects.machines(machine_uuid, name) VALUES ($1, $2)
         ON CONFLICT(machine_uuid) DO NOTHING",
        machine,
        name
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn colliding_locations(
    tx: &mut Tx<'_>,
    legacy: Uuid,
    current: Uuid,
) -> Result<Pairs, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT CASE WHEN l.last_seen > c.last_seen THEN c.location_uuid ELSE l.location_uuid END AS "loser!",
                CASE WHEN l.last_seen > c.last_seen THEN l.location_uuid ELSE c.location_uuid END AS "winner!"
           FROM projects.locations l
           JOIN projects.locations c ON c.root_path = l.root_path AND c.machine_uuid = $2
          WHERE l.machine_uuid = $1
            FOR UPDATE OF l, c"#,
        legacy,
        current
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(Pairs::from_rows(
        rows.into_iter()
            .map(|row| (row.loser, row.winner))
            .collect(),
    ))
}

async fn merge_collisions(tx: &mut Tx<'_>, pairs: &Pairs) -> Result<CollisionCounts, sqlx::Error> {
    if pairs.is_empty() {
        return Ok(CollisionCounts::default());
    }
    let sessions = repoint_sessions(tx, pairs).await?;
    let (sources, documents) = merge_sources(tx, pairs).await?;
    merge_lesson_files(tx, pairs).await?;
    sqlx::query!(
        "DELETE FROM projects.locations WHERE location_uuid = ANY($1)",
        &pairs.losers[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(CollisionCounts {
        sessions,
        sources,
        documents,
    })
}

async fn repoint_sessions(tx: &mut Tx<'_>, pairs: &Pairs) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE sessions.sessions s SET location_uuid = p.winner
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
          WHERE s.location_uuid = p.loser",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

async fn merge_sources(tx: &mut Tx<'_>, pairs: &Pairs) -> Result<(u64, u64), sqlx::Error> {
    sqlx::query!(
        "UPDATE memory.knowledge_sources s SET location_uuid = p.winner
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
          WHERE s.location_uuid = p.loser
            AND NOT EXISTS (SELECT 1 FROM memory.knowledge_sources w
                             WHERE w.location_uuid = p.winner AND w.name = s.name)",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT d.source_uuid AS "dropped!", w.source_uuid AS "kept!"
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
           JOIN memory.knowledge_sources d ON d.location_uuid = p.loser
           JOIN memory.knowledge_sources w ON w.location_uuid = p.winner AND w.name = d.name"#,
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .fetch_all(&mut **tx)
    .await?;
    let sources = Pairs::from_rows(
        rows.into_iter()
            .map(|row| (row.dropped, row.kept))
            .collect(),
    );
    if sources.is_empty() {
        return Ok((0, 0));
    }
    let documents = merge_documents(tx, &sources).await?;
    Ok((sources.len(), documents))
}

async fn merge_documents(tx: &mut Tx<'_>, sources: &Pairs) -> Result<u64, sqlx::Error> {
    sqlx::query!(
        "UPDATE memory.knowledge_revisions r SET document_uuid = k.document_uuid
           FROM unnest($1::uuid[], $2::uuid[]) AS m(dropped, kept)
           JOIN memory.knowledge_documents dd ON dd.source_uuid = m.dropped
           JOIN memory.knowledge_documents k
             ON k.source_uuid = m.kept AND k.relative_path = dd.relative_path
          WHERE r.document_uuid = dd.document_uuid",
        &sources.losers[..],
        &sources.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    let duplicates = sqlx::query!(
        "DELETE FROM memory.knowledge_documents dd
          USING unnest($1::uuid[], $2::uuid[]) AS m(dropped, kept),
                memory.knowledge_documents k
          WHERE dd.source_uuid = m.dropped
            AND k.source_uuid = m.kept AND k.relative_path = dd.relative_path",
        &sources.losers[..],
        &sources.winners[..]
    )
    .execute(&mut **tx)
    .await?
    .rows_affected();
    sqlx::query!(
        "UPDATE memory.knowledge_documents d SET source_uuid = m.kept
           FROM unnest($1::uuid[], $2::uuid[]) AS m(dropped, kept)
          WHERE d.source_uuid = m.dropped",
        &sources.losers[..],
        &sources.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "DELETE FROM memory.knowledge_sources WHERE source_uuid = ANY($1)",
        &sources.losers[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(duplicates)
}

async fn merge_lesson_files(tx: &mut Tx<'_>, pairs: &Pairs) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM memory.lesson_file_receipts w
          USING unnest($1::uuid[], $2::uuid[]) AS p(loser, winner),
                memory.lesson_file_receipts l
          WHERE w.location_uuid = p.winner AND l.location_uuid = p.loser
            AND l.lesson_uuid = w.lesson_uuid
            AND (l.version, l.written_at) > (w.version, w.written_at)",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "DELETE FROM memory.lesson_file_receipts l
          USING unnest($1::uuid[], $2::uuid[]) AS p(loser, winner),
                memory.lesson_file_receipts w
          WHERE l.location_uuid = p.loser AND w.location_uuid = p.winner
            AND w.lesson_uuid = l.lesson_uuid",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE memory.lesson_file_receipts r SET location_uuid = p.winner
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
          WHERE r.location_uuid = p.loser",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE memory.lesson_file_conflicts c SET location_uuid = p.winner
           FROM unnest($1::uuid[], $2::uuid[]) AS p(loser, winner)
          WHERE c.location_uuid = p.loser
            AND NOT EXISTS (SELECT 1 FROM memory.lesson_file_conflicts w
                             WHERE w.location_uuid = p.winner AND w.lesson_uuid = c.lesson_uuid)",
        &pairs.losers[..],
        &pairs.winners[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn move_locations(tx: &mut Tx<'_>, legacy: Uuid, current: Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE projects.locations SET machine_uuid = $2 WHERE machine_uuid = $1",
        legacy,
        current
    )
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

async fn retag_llm_calls(tx: &mut Tx<'_>, legacy: Uuid, current: Uuid) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE llm.calls SET machine_uuid = $2 WHERE machine_uuid = $1",
        legacy,
        current
    )
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

async fn retag_worker_leases(
    tx: &mut Tx<'_>,
    legacy: Uuid,
    current: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE llm.worker_leases SET machine_uuid = $2 WHERE machine_uuid = $1",
        legacy,
        current
    )
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

#[cfg(test)]
#[path = "machine_merge_db_tests.rs"]
mod db_tests;
