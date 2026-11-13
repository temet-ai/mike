use sha2::{Digest, Sha256};
use sqlx::PgConnection;

use super::error::PersistError;
use super::messages::{hex_lowercase, invariant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireTool {
    pub sha: String,
    pub body_wire_bytes: Vec<u8>,
    pub byte_size: i32,
    pub tool_name: String,
    pub ord: i32,
}

pub fn extract_body_tools(body_bytes: &[u8]) -> Result<Vec<WireTool>, PersistError> {
    #[derive(serde::Deserialize)]
    struct Shape<'a> {
        #[serde(borrow, default)]
        tools: Vec<&'a serde_json::value::RawValue>,
    }
    #[derive(serde::Deserialize)]
    struct NameOnly {
        name: String,
    }

    let parsed: Shape = serde_json::from_slice(body_bytes)?;

    let mut out = Vec::with_capacity(parsed.tools.len());
    for (ord, item) in parsed.tools.iter().enumerate() {
        let wire_text = item.get();
        let NameOnly { name: tool_name } = serde_json::from_str(wire_text)
            .map_err(|_| invariant(format!("tools[{ord}] missing required string field `name`")))?;

        let wire_bytes = wire_text.as_bytes().to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&wire_bytes);
        let sha = hex_lowercase(&hasher.finalize());
        let byte_size: i32 = wire_bytes.len().try_into().map_err(|_| {
            invariant(format!(
                "tools[{ord}] body exceeds INT32_MAX bytes ({} bytes)",
                wire_bytes.len()
            ))
        })?;

        out.push(WireTool {
            sha,
            body_wire_bytes: wire_bytes,
            byte_size,
            tool_name,
            ord: i32::try_from(ord).map_err(|_| {
                invariant(format!(
                    "tools[] contains > INT32_MAX entries ({ord} reached)"
                ))
            })?,
        });
    }
    Ok(out)
}

struct WireColumns {
    ords: Vec<i32>,
    names: Vec<String>,
    shas: Vec<String>,
    bodies: Vec<Vec<u8>>,
    sizes: Vec<i32>,
}

impl WireColumns {
    fn from_tools(tools: &[WireTool]) -> Self {
        Self {
            ords: tools.iter().map(|t| t.ord).collect(),
            names: tools.iter().map(|t| t.tool_name.clone()).collect(),
            shas: tools.iter().map(|t| t.sha.clone()).collect(),
            bodies: tools.iter().map(|t| t.body_wire_bytes.clone()).collect(),
            sizes: tools.iter().map(|t| t.byte_size).collect(),
        }
    }
}

pub async fn write_request_tools(
    conn: &mut PgConnection,
    request_id: i64,
    session_id: i64,
    tools: &[WireTool],
) -> Result<(), PersistError> {
    if tools.is_empty() {
        return Ok(());
    }
    let wire = WireColumns::from_tools(tools);
    sqlx::query!(
        "WITH wire AS (
             SELECT u.ord, u.name, u.sha, u.body, u.byte_size
               FROM UNNEST($2::int[], $3::text[], $4::text[], $5::bytea[], $6::int[])
                    AS u(ord, name, sha, body, byte_size)
              WHERE NOT EXISTS (SELECT 1 FROM requests.tools rt WHERE rt.request_id = $1)
         ),
         distinct_wire AS (
             SELECT DISTINCT ON (sha) sha, name, body, byte_size
               FROM wire
              ORDER BY sha, ord
         ),
         recent_request AS (
             SELECT r.id
               FROM requests.requests r
              WHERE r.session_id = $7
                AND r.id <> $1
                AND EXISTS (SELECT 1 FROM requests.tools rt WHERE rt.request_id = r.id)
              ORDER BY r.created_at DESC, r.id DESC
              LIMIT 3
         ),
         recent_definition AS (
             SELECT DISTINCT ON (d.definition_sha256) d.definition_sha256 AS sha, d.id
               FROM tools.definitions d
              WHERE d.id IN (
                        SELECT rt.definition_id
                          FROM recent_request rr
                          JOIN requests.tools rt ON rt.request_id = rr.id
                    )
                AND d.definition_sha256 IN (SELECT sha FROM distinct_wire)
              ORDER BY d.definition_sha256, d.id
         ),
         unmatched AS (
             SELECT dw.sha, dw.name, dw.body, dw.byte_size
               FROM distinct_wire dw
              WHERE NOT EXISTS (SELECT 1 FROM recent_definition k WHERE k.sha = dw.sha)
         ),
         stored_definition AS (
             SELECT DISTINCT ON (d.definition_sha256) d.definition_sha256 AS sha, d.id
               FROM tools.definitions d
              WHERE EXISTS (SELECT 1 FROM unmatched)
                AND d.definition_sha256 IN (SELECT sha FROM unmatched)
              ORDER BY d.definition_sha256, d.id
         ),
         created AS (
             INSERT INTO tools.definitions (definition_sha256, tool_name, body, byte_size)
             SELECT um.sha, um.name, um.body, um.byte_size
               FROM unmatched um
              WHERE NOT EXISTS (SELECT 1 FROM stored_definition s WHERE s.sha = um.sha)
             RETURNING id, definition_sha256 AS sha
         ),
         definition AS (
             SELECT sha, id FROM recent_definition
             UNION ALL
             SELECT sha, id FROM stored_definition
             UNION ALL
             SELECT sha, id FROM created
         )
         INSERT INTO requests.tools (request_id, ord, tool_name, definition_id)
         SELECT $1, wire.ord, wire.name, definition.id
           FROM wire
           JOIN definition ON definition.sha = wire.sha",
        request_id,
        &wire.ords,
        &wire.names,
        &wire.shas,
        &wire.bodies,
        &wire.sizes,
        session_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn backfill_apply_definitions(
    conn: &mut PgConnection,
    request_id: i64,
    body_bytes: &[u8],
) -> Result<usize, PersistError> {
    let extracted = extract_body_tools(body_bytes)?;
    if extracted.is_empty() {
        return Ok(0);
    }
    let wire = WireColumns::from_tools(&extracted);
    let result = sqlx::query!(
        "WITH wire AS (
             SELECT u.ord, u.name, u.sha, u.body, u.byte_size
               FROM UNNEST($2::int[], $3::text[], $4::text[], $5::bytea[], $6::int[])
                    AS u(ord, name, sha, body, byte_size)
         ),
         distinct_wire AS (
             SELECT DISTINCT ON (sha) sha, name, body, byte_size
               FROM wire
              ORDER BY sha, ord
         ),
         known AS (
             SELECT DISTINCT ON (d.definition_sha256) d.definition_sha256 AS sha, d.id
               FROM tools.definitions d
              WHERE d.definition_sha256 IN (SELECT sha FROM distinct_wire)
              ORDER BY d.definition_sha256, d.id
         ),
         created AS (
             INSERT INTO tools.definitions (definition_sha256, tool_name, body, byte_size)
             SELECT dw.sha, dw.name, dw.body, dw.byte_size
               FROM distinct_wire dw
              WHERE NOT EXISTS (SELECT 1 FROM known k WHERE k.sha = dw.sha)
             RETURNING id, definition_sha256 AS sha
         ),
         definition AS (
             SELECT sha, id FROM known
             UNION ALL
             SELECT sha, id FROM created
         )
         UPDATE requests.tools rt
            SET definition_id = definition.id
           FROM wire
           JOIN definition ON definition.sha = wire.sha
          WHERE rt.request_id = $1
            AND rt.ord = wire.ord
            AND rt.definition_id IS NULL",
        request_id,
        &wire.ords,
        &wire.names,
        &wire.shas,
        &wire.bodies,
        &wire.sizes,
    )
    .execute(&mut *conn)
    .await?;

    Ok(result.rows_affected() as usize)
}

#[cfg(test)]
mod tests;
