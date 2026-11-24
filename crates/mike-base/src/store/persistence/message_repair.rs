use std::collections::HashMap;

use sqlx::{PgConnection, PgPool};

use super::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageCopy {
    pub copy_id: i64,
    pub keep_id: i64,
    pub byte_size: i32,
    pub month: i32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BatchRepair {
    pub copies: usize,
    pub repaired: usize,
    pub references_moved: u64,
    pub rows_deleted: u64,
    pub bytes_deleted: i64,
    pub requests_checked: usize,
    pub mismatched_requests: Vec<i64>,
}

pub async fn find_message_copies(pool: &PgPool) -> Result<Vec<MessageCopy>, PersistError> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!("SELECT set_config('statement_timeout', '3h', true)")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query_scalar!("SELECT set_config('work_mem', '512MB', true)")
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query!(
        "SELECT x.id AS \"copy_id!\", x.keep_id AS \"keep_id!\", x.byte_size AS \"byte_size!\",
                (extract(year FROM x.first_seen) * 100 + extract(month FROM x.first_seen))::int
                    AS \"month!\"
           FROM (SELECT m.id, m.byte_size, m.first_seen,
                        first_value(m.id) OVER (PARTITION BY m.message_sha256 ORDER BY m.id) AS keep_id
                   FROM messages.messages m) x
          WHERE x.id <> x.keep_id
          ORDER BY x.id"
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.rollback().await?;
    Ok(rows
        .into_iter()
        .map(|r| MessageCopy {
            copy_id: r.copy_id,
            keep_id: r.keep_id,
            byte_size: r.byte_size,
            month: r.month,
        })
        .collect())
}

pub async fn repair_message_copies(
    pool: &PgPool,
    batch: &[MessageCopy],
    idle_minutes: i32,
) -> Result<BatchRepair, PersistError> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!("SELECT set_config('statement_timeout', '10min', true)")
        .fetch_one(&mut *tx)
        .await?;
    let (copies, keeps) = identical_idle_copies(&mut tx, batch, idle_minutes).await?;
    let mut repair = BatchRepair {
        copies: batch.len(),
        repaired: copies.len(),
        ..BatchRepair::default()
    };
    if copies.is_empty() {
        tx.rollback().await?;
        return Ok(repair);
    }
    let requests = sample_requests(&mut tx, &copies).await?;
    repair.requests_checked = requests.len();
    let before = fingerprints(&mut tx, &requests).await?;
    repair.references_moved = repoint(&mut tx, &copies, &keeps).await?;
    let after = fingerprints(&mut tx, &requests).await?;
    repair.mismatched_requests = mismatched(&before, &after);
    if !repair.mismatched_requests.is_empty() {
        tx.rollback().await?;
        return Ok(repair);
    }
    (repair.rows_deleted, repair.bytes_deleted) = delete_unreferenced(&mut tx, &copies).await?;
    tx.commit().await?;
    Ok(repair)
}

async fn identical_idle_copies(
    conn: &mut PgConnection,
    batch: &[MessageCopy],
    idle_minutes: i32,
) -> Result<(Vec<i64>, Vec<i64>), PersistError> {
    let copy_ids: Vec<i64> = batch.iter().map(|c| c.copy_id).collect();
    let keep_ids: Vec<i64> = batch.iter().map(|c| c.keep_id).collect();
    let rows = sqlx::query!(
        "WITH map AS (
             SELECT * FROM UNNEST($1::bigint[], $2::bigint[]) AS u(copy_id, keep_id)
         ),
         busy AS (
             SELECT rm.message_id AS copy_id
               FROM requests.messages rm
               JOIN requests.requests r ON r.id = rm.request_id
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE rm.message_id = ANY($1)
                AND s.last_seen >= now() - make_interval(mins => $3)
             UNION
             SELECT sm.message_id
               FROM requests.sent_messages sm
               JOIN requests.requests r ON r.id = sm.request_id
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE sm.message_id = ANY($1)
                AND s.last_seen >= now() - make_interval(mins => $3)
             UNION
             SELECT d.message_id
               FROM requests.deltas d
               JOIN sessions.sessions s ON s.id = d.session_id
              WHERE d.message_id = ANY($1)
                AND s.last_seen >= now() - make_interval(mins => $3)
         )
         SELECT map.copy_id AS \"copy_id!\", map.keep_id AS \"keep_id!\"
           FROM map
           JOIN messages.messages c ON c.id = map.copy_id
           JOIN messages.messages k ON k.id = map.keep_id
          WHERE c.role_id = k.role_id
            AND c.body::text = k.body::text
            AND NOT EXISTS (SELECT 1 FROM busy WHERE busy.copy_id = map.copy_id)
          ORDER BY map.copy_id",
        &copy_ids,
        &keep_ids,
        idle_minutes,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.copy_id, r.keep_id)).unzip())
}

async fn sample_requests(
    conn: &mut PgConnection,
    copies: &[i64],
) -> Result<Vec<i64>, PersistError> {
    Ok(sqlx::query_scalar!(
        "SELECT id AS \"id!\" FROM (
             (SELECT rm.request_id AS id FROM requests.messages rm
               WHERE rm.message_id = ANY($1) ORDER BY rm.request_id LIMIT 20)
             UNION
             (SELECT sm.request_id FROM requests.sent_messages sm
               WHERE sm.message_id = ANY($1) ORDER BY sm.request_id LIMIT 10)
             UNION
             (SELECT d.request_id FROM requests.deltas d
               WHERE d.message_id = ANY($1) ORDER BY d.request_id LIMIT 10)
         ) sampled
         ORDER BY id",
        copies,
    )
    .fetch_all(&mut *conn)
    .await?)
}

async fn fingerprints(
    conn: &mut PgConnection,
    requests: &[i64],
) -> Result<HashMap<(i64, String), String>, PersistError> {
    let rows = sqlx::query!(
        "SELECT rm.request_id AS \"request_id!\", 'messages' AS \"part!\",
                md5(string_agg(rm.ord || ':' || m.body::text || ':' || coalesce(rm.cache_control::text, ''),
                               E'\\n' ORDER BY rm.ord, rm.id)) AS \"digest!\"
           FROM requests.messages rm JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = ANY($1)
          GROUP BY rm.request_id
         UNION ALL
         SELECT sm.request_id, 'sent',
                md5(string_agg(sm.ord || ':' || m.body::text || ':' || coalesce(sm.cache_control::text, ''),
                               E'\\n' ORDER BY sm.ord))
           FROM requests.sent_messages sm JOIN messages.messages m ON m.id = sm.message_id
          WHERE sm.request_id = ANY($1)
          GROUP BY sm.request_id
         UNION ALL
         SELECT d.request_id, 'deltas',
                md5(string_agg(d.ord || ':' || d.change_kind_id || ':' || m.body::text,
                               E'\\n' ORDER BY d.change_kind_id, d.ord, d.id))
           FROM requests.deltas d JOIN messages.messages m ON m.id = d.message_id
          WHERE d.request_id = ANY($1)
          GROUP BY d.request_id",
        requests,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ((r.request_id, r.part), r.digest))
        .collect())
}

async fn repoint(
    conn: &mut PgConnection,
    copies: &[i64],
    keeps: &[i64],
) -> Result<u64, PersistError> {
    let messages = sqlx::query!(
        "UPDATE requests.messages rm
            SET message_id = map.keep_id
           FROM UNNEST($1::bigint[], $2::bigint[]) AS map(copy_id, keep_id)
          WHERE rm.message_id = map.copy_id",
        copies,
        keeps,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let sent = sqlx::query!(
        "UPDATE requests.sent_messages sm
            SET message_id = map.keep_id
           FROM UNNEST($1::bigint[], $2::bigint[]) AS map(copy_id, keep_id)
          WHERE sm.message_id = map.copy_id",
        copies,
        keeps,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let deltas = sqlx::query!(
        "UPDATE requests.deltas d
            SET message_id = map.keep_id
           FROM UNNEST($1::bigint[], $2::bigint[]) AS map(copy_id, keep_id)
          WHERE d.message_id = map.copy_id",
        copies,
        keeps,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(messages + sent + deltas)
}

fn mismatched(
    before: &HashMap<(i64, String), String>,
    after: &HashMap<(i64, String), String>,
) -> Vec<i64> {
    let mut requests: Vec<i64> = before
        .iter()
        .filter(|(key, digest)| after.get(*key) != Some(*digest))
        .map(|((request, _), _)| *request)
        .chain(
            after
                .keys()
                .filter(|key| !before.contains_key(*key))
                .map(|(request, _)| *request),
        )
        .collect();
    requests.sort_unstable();
    requests.dedup();
    requests
}

async fn delete_unreferenced(
    conn: &mut PgConnection,
    copies: &[i64],
) -> Result<(u64, i64), PersistError> {
    let row = sqlx::query!(
        "WITH gone AS (
             DELETE FROM messages.messages m
              WHERE m.id = ANY($1)
                AND NOT EXISTS (SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id)
                AND NOT EXISTS (SELECT 1 FROM requests.sent_messages sm WHERE sm.message_id = m.id)
                AND NOT EXISTS (SELECT 1 FROM requests.deltas d WHERE d.message_id = m.id)
             RETURNING m.byte_size
         )
         SELECT count(*) AS \"rows!\", coalesce(sum(byte_size), 0)::bigint AS \"bytes!\" FROM gone",
        copies,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((u64::try_from(row.rows).unwrap_or(0), row.bytes))
}
