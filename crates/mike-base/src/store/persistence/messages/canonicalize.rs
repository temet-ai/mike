use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::collections::HashMap;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMessage {
    pub sha: String,
    pub body_canonical_bytes: Vec<u8>,
    pub byte_size: i32,
    pub role: String,
    pub ord: i32,
    pub cache_control: Option<JsonValue>,
}

pub fn invariant(msg: impl Into<String>) -> PersistError {
    PersistError::DataInvariant(msg.into())
}

pub fn canonicalise_body_messages(
    body_bytes: &[u8],
) -> Result<Vec<CanonicalMessage>, PersistError> {
    let mut value: JsonValue = serde_json::from_slice(body_bytes)?;
    let Some(messages) = value.as_object_mut().and_then(|o| o.remove("messages")) else {
        return Ok(Vec::new());
    };
    let JsonValue::Array(items) = messages else {
        return Err(invariant(
            "request_body.messages was present but not a JSON array",
        ));
    };

    let mut out = Vec::with_capacity(items.len());
    for (ord, mut item) in items.into_iter().enumerate() {
        let mut cache_control_hits = Vec::new();
        strip_and_record_cache_control(&mut item, "", &mut cache_control_hits);
        let cache_control = if cache_control_hits.is_empty() {
            None
        } else {
            Some(JsonValue::Array(
                cache_control_hits
                    .into_iter()
                    .map(|(pointer, value)| serde_json::json!({"pointer": pointer, "value": value}))
                    .collect(),
            ))
        };

        let role = item
            .get("role")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                invariant(format!(
                    "messages[{ord}] missing required string field `role`"
                ))
            })?
            .to_owned();
        if role != "user" && role != "assistant" && role != "system" {
            return Err(invariant(format!(
                "messages[{ord}].role = {role:?} is not in {{user, assistant, system}}"
            )));
        }

        let mut canonical = Vec::with_capacity(256);
        write_canonical(&mut canonical, &item)?;

        let mut hasher = Sha256::new();
        hasher.update(&canonical);
        let sha_bytes = hasher.finalize();
        let sha = hex_lowercase(&sha_bytes);
        let byte_size: i32 = canonical.len().try_into().map_err(|_| {
            invariant(format!(
                "messages[{ord}] canonical body exceeds INT32_MAX bytes ({} bytes)",
                canonical.len()
            ))
        })?;

        out.push(CanonicalMessage {
            sha,
            body_canonical_bytes: canonical,
            byte_size,
            role,
            ord: i32::try_from(ord).map_err(|_| {
                invariant(format!(
                    "messages[] contains > INT32_MAX entries ({ord} reached)"
                ))
            })?,
            cache_control,
        });
    }
    Ok(out)
}

fn strip_and_record_cache_control(
    value: &mut JsonValue,
    pointer: &str,
    out: &mut Vec<(String, JsonValue)>,
) {
    match value {
        JsonValue::Object(map) => {
            if let Some(cc) = map.remove("cache_control") {
                out.push((format!("{pointer}/cache_control"), cc));
            }
            for (key, v) in map.iter_mut() {
                let child_pointer = format!("{pointer}/{}", escape_pointer_segment(key));
                strip_and_record_cache_control(v, &child_pointer, out);
            }
        }
        JsonValue::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                strip_and_record_cache_control(v, &format!("{pointer}/{i}"), out);
            }
        }
        _ => {}
    }
}

fn escape_pointer_segment(key: &str) -> String {
    if key.contains('~') || key.contains('/') {
        key.replace('~', "~0").replace('/', "~1")
    } else {
        key.to_string()
    }
}

pub fn write_canonical(
    buf: &mut Vec<u8>,
    value: &JsonValue,
) -> Result<(), serde_json::Error> {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => {
            serde_json::to_writer(buf, value)
        }
        JsonValue::Array(arr) => {
            buf.push(b'[');
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                write_canonical(buf, v)?;
            }
            buf.push(b']');
            Ok(())
        }
        JsonValue::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            buf.push(b'{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                serde_json::to_writer(&mut *buf, key)?;
                buf.push(b':');
                let Some(value) = map.get(*key) else {
                    continue;
                };
                write_canonical(buf, value)?;
            }
            buf.push(b'}');
            Ok(())
        }
    }
}

pub fn hex_lowercase(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

struct NewMessageBatch {
    shas: Vec<String>,
    bodies: Vec<JsonValue>,
    byte_sizes: Vec<i32>,
    roles: Vec<String>,
}

fn collect_new_message_bodies(
    canonical: &[CanonicalMessage],
    carry_forward: &HashMap<String, i64>,
) -> Result<NewMessageBatch, serde_json::Error> {
    use std::collections::HashSet;

    let mut seen_new: HashSet<&str> = HashSet::with_capacity(canonical.len());
    let mut batch = NewMessageBatch {
        shas: Vec::new(),
        bodies: Vec::new(),
        byte_sizes: Vec::new(),
        roles: Vec::new(),
    };
    for m in canonical {
        if carry_forward.contains_key(&m.sha) || !seen_new.insert(m.sha.as_str()) {
            continue;
        }
        batch.shas.push(m.sha.clone());
        batch
            .bodies
            .push(serde_json::from_slice(&m.body_canonical_bytes)?);
        batch.byte_sizes.push(m.byte_size);
        batch.roles.push(m.role.clone());
    }
    Ok(batch)
}

struct BridgeRows {
    request_ids: Vec<i64>,
    ords: Vec<i32>,
    message_ids: Vec<i64>,
    cache_controls: Vec<Option<JsonValue>>,
}

fn resolve_bridge_rows(
    request_id: i64,
    canonical: &[CanonicalMessage],
    carry_forward: &HashMap<String, i64>,
    new_ids: &HashMap<String, i64>,
) -> Result<BridgeRows, PersistError> {
    let mut rows = BridgeRows {
        request_ids: vec![request_id; canonical.len()],
        ords: Vec::with_capacity(canonical.len()),
        message_ids: Vec::with_capacity(canonical.len()),
        cache_controls: Vec::with_capacity(canonical.len()),
    };
    for m in canonical {
        let message_id = carry_forward
            .get(&m.sha)
            .or_else(|| new_ids.get(&m.sha))
            .copied()
            .ok_or_else(|| {
                invariant(format!(
                    "messages[{}] sha {} resolved to no message_id \
                     (carry-forward and insert both missed)",
                    m.ord, m.sha
                ))
            })?;
        rows.ords.push(m.ord);
        rows.message_ids.push(message_id);
        rows.cache_controls.push(m.cache_control.clone());
    }
    Ok(rows)
}

pub async fn write_request_messages(
    conn: &mut PgConnection,
    request_id: i64,
    canonical: &[CanonicalMessage],
    carry_forward: &HashMap<String, i64>,
) -> Result<(), PersistError> {
    if canonical.is_empty() {
        return Ok(());
    }

    let already_bridged: Option<i32> = sqlx::query_scalar!(
        "SELECT 1 AS \"one!\" FROM requests.messages WHERE request_id = $1 LIMIT 1",
        request_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if already_bridged.is_some() {
        return Ok(());
    }

    let new = collect_new_message_bodies(canonical, carry_forward)?;

    let mut new_ids: HashMap<String, i64> = HashMap::with_capacity(new.shas.len());
    if !new.shas.is_empty() {
        let inserted = sqlx::query!(
            "INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT u.sha, u.body, u.byte_size, r.id
             FROM UNNEST($1::text[], $2::jsonb[], $3::int[], $4::text[])
                  AS u(sha, body, byte_size, role)
             JOIN messages.roles r ON r.code = u.role
             RETURNING id AS \"id!\", message_sha256 AS \"message_sha256!\"",
            &new.shas,
            &new.bodies,
            &new.byte_sizes,
            &new.roles,
        )
        .fetch_all(&mut *conn)
        .await?;
        for row in inserted {
            new_ids.insert(row.message_sha256, row.id);
        }
    }

    let bridge = resolve_bridge_rows(request_id, canonical, carry_forward, &new_ids)?;

    sqlx::query!(
        "INSERT INTO requests.messages (request_id, ord, message_id, cache_control)
         SELECT u.request_id, u.ord, u.message_id, u.cache_control
         FROM UNNEST($1::bigint[], $2::int[], $3::bigint[], $4::jsonb[])
              AS u(request_id, ord, message_id, cache_control)",
        &bridge.request_ids,
        &bridge.ords,
        &bridge.message_ids,
        &bridge.cache_controls as &[Option<JsonValue>],
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}
