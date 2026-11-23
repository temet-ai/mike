use super::{CaptureChunkInput, HashMap, PersistError, PgConnection, invalid};

pub(super) async fn validate_references(
    connection: &mut PgConnection,
    predecessor: Option<i64>,
    chunks: &[CaptureChunkInput],
) -> Result<(), PersistError> {
    let mut ids: Vec<_> = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            CaptureChunkInput::Existing(id) => Some(*id),
            CaptureChunkInput::New(_) => None,
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }
    let matched = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT chunk_id) AS "count!" FROM runtime.capture_document_chunks
           WHERE document_id = $1 AND chunk_id = ANY($2)"#,
        predecessor,
        &ids
    )
    .fetch_one(connection)
    .await?;
    if matched != ids.len() as i64 {
        return Err(invalid(
            "reused capture chunk is not in the known predecessor document",
        ));
    }
    Ok(())
}

pub(super) async fn insert_chunks(
    connection: &mut PgConnection,
    document: i64,
    start: i64,
    chunks: &[CaptureChunkInput],
) -> Result<Vec<i64>, PersistError> {
    let new: Vec<_> = chunks
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| match chunk {
            CaptureChunkInput::New(body) => Some((start + index as i64, body.clone())),
            CaptureChunkInput::Existing(_) => None,
        })
        .collect();
    let (ordinals, bodies): (Vec<_>, Vec<_>) = new.into_iter().unzip();
    let ids: HashMap<_, _> = sqlx::query!(
        "INSERT INTO runtime.capture_chunks (origin_document_id, origin_ordinal, body)
         SELECT $1, input.ordinal, input.body FROM UNNEST($2::bigint[], $3::bytea[]) AS input(ordinal, body)
         RETURNING id, origin_ordinal",
        document, &ordinals, &bodies
    ).fetch_all(connection).await?.into_iter().map(|row| (row.origin_ordinal, row.id)).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| match chunk {
            CaptureChunkInput::Existing(id) => Ok(*id),
            CaptureChunkInput::New(_) => ids
                .get(&(start + index as i64))
                .copied()
                .ok_or_else(|| invalid("inserted capture chunk did not return its id")),
        })
        .collect()
}

pub(super) async fn insert_references(
    connection: &mut PgConnection,
    document: i64,
    start: i64,
    ids: &[i64],
) -> Result<(), PersistError> {
    sqlx::query!(
        "WITH inserted AS (
            INSERT INTO runtime.capture_document_chunks (document_id, ordinal, chunk_id)
            SELECT $1, $2 + input.ord - 1, input.id FROM UNNEST($3::bigint[]) WITH ORDINALITY AS input(id, ord)
            RETURNING chunk_id
         ), sizes AS (
            SELECT count(*) AS chunks, COALESCE(sum(c.byte_size), 0)::bigint AS bytes
            FROM inserted i JOIN runtime.capture_chunks c ON c.id = i.chunk_id
         ) UPDATE runtime.capture_documents d SET chunk_count = d.chunk_count + sizes.chunks,
              byte_size = d.byte_size + sizes.bytes FROM sizes WHERE d.id = $1",
        document, start, ids
    ).execute(connection).await?;
    Ok(())
}
