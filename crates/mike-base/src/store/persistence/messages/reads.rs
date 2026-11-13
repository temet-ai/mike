use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgConnection;
use std::collections::HashMap;

use crate::store::persistence::PersistError;

use super::canonicalize::CanonicalMessage;
use super::delta::{DeltaRow, compute_delta, is_reliable_sidecar_origin};

#[derive(sqlx::FromRow)]
struct PredecessorRef {
    id: i64,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct BridgeRef {
    ord: i32,
    message_sha256: String,
    message_id: i64,
}

pub async fn fetch_canonical_bridge(
    conn: &mut PgConnection,
    request_id: i64,
) -> Result<Vec<CanonicalMessage>, PersistError> {
    struct Row {
        ord: i32,
        sha: String,
        body: JsonValue,
    }
    let rows = sqlx::query_as!(
        Row,
        "SELECT rm.ord, m.message_sha256 AS \"sha!\", m.body AS \"body!\"
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = $1
          ORDER BY rm.ord ASC",
        request_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let body_canonical_bytes = serde_json::to_vec(&r.body).unwrap_or_default();
            CanonicalMessage {
                byte_size: body_canonical_bytes.len() as i32,
                body_canonical_bytes,
                sha: r.sha,
                role: String::new(),
                ord: r.ord,
                cache_control: None,
            }
        })
        .collect())
}

pub async fn fetch_predecessor_primary(
    conn: &mut PgConnection,
    session_db_id: i64,
    created_at: DateTime<Utc>,
    agent_instance_id: Option<i64>,
) -> Result<Option<(i64, DateTime<Utc>)>, PersistError> {
    match agent_instance_id {
        Some(instance_id) => {
            let row = sqlx::query_as!(
                PredecessorRef,
                "SELECT r.id, r.created_at
                   FROM requests.requests r
                   JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code = 'subagent'
                  WHERE r.session_id = $1
                    AND r.agent_instance_id = $3
                    AND r.created_at < $2
                  ORDER BY r.created_at DESC, r.id DESC
                  LIMIT 1",
                session_db_id,
                created_at,
                instance_id,
            )
            .fetch_optional(&mut *conn)
            .await?;
            Ok(row.map(|r| (r.id, r.created_at)))
        }
        None => {
            let row = sqlx::query_as!(
                PredecessorRef,
                "SELECT r.id, r.created_at
                   FROM requests.requests r
                   JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code IN ('primary', 'primary_first')
                  WHERE r.session_id = $1
                    AND r.agent_instance_id IS NULL
                    AND r.created_at < $2
                  ORDER BY r.created_at DESC, r.id DESC
                  LIMIT 1",
                session_db_id,
                created_at,
            )
            .fetch_optional(&mut *conn)
            .await?;
            Ok(row.map(|r| (r.id, r.created_at)))
        }
    }
}

pub async fn fetch_predecessor_bridge(
    conn: &mut PgConnection,
    predecessor_request_id: i64,
) -> Result<Vec<(i32, String, i64)>, PersistError> {
    let rows = sqlx::query_as!(
        BridgeRef,
        "SELECT rm.ord, rm.message_id, m.message_sha256 AS \"message_sha256!\"
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = $1
          ORDER BY rm.ord ASC",
        predecessor_request_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.ord, r.message_sha256, r.message_id))
        .collect())
}

pub struct PredecessorDelta {
    pub predecessor_request_id: Option<i64>,
    pub rows: Vec<DeltaRow>,
    pub carry_forward: HashMap<String, i64>,
}

pub async fn resolve_predecessor_and_delta(
    conn: &mut PgConnection,
    session_db_id: i64,
    created_at: DateTime<Utc>,
    turn_origin: &str,
    canonical: &[CanonicalMessage],
    agent_instance_id: Option<i64>,
) -> Result<PredecessorDelta, PersistError> {
    if is_reliable_sidecar_origin(turn_origin) {
        let rows = compute_delta(canonical, None);
        return Ok(PredecessorDelta {
            predecessor_request_id: None,
            rows,
            carry_forward: HashMap::new(),
        });
    }

    let predecessor =
        fetch_predecessor_primary(conn, session_db_id, created_at, agent_instance_id).await?;
    let pred_bridge: Vec<(i32, String, i64)> = match predecessor {
        Some((pid, _pca)) => fetch_predecessor_bridge(conn, pid).await?,
        None => Vec::new(),
    };
    let carry_forward: HashMap<String, i64> = pred_bridge
        .iter()
        .map(|(_, sha, message_id)| (sha.clone(), *message_id))
        .collect();
    let pred_shas: Vec<(i32, String)> = pred_bridge
        .iter()
        .map(|(ord, sha, _)| (*ord, sha.clone()))
        .collect();
    let pred_bridge_ref: Option<&[(i32, String)]> = predecessor.map(|_| pred_shas.as_slice());
    let rows = compute_delta(canonical, pred_bridge_ref);
    Ok(PredecessorDelta {
        predecessor_request_id: predecessor.map(|(pid, _)| pid),
        rows,
        carry_forward,
    })
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PersistedDeltaRow {
    pub request_id: i64,
    pub predecessor_request_id: Option<i64>,
    pub change_kind: String,
    pub ord: i32,
    pub message_id: i64,
    pub message_sha256: String,
    pub role: String,
    pub byte_size: i32,
    pub body: serde_json::Value,
    pub removed_reason: Option<String>,
    pub computed_at: DateTime<Utc>,
}

pub async fn fetch_request_deltas_by_id(
    pool: &sqlx::PgPool,
    request_id: i64,
    session_db_id: i64,
) -> Result<Vec<PersistedDeltaRow>, PersistError> {
    let rows: Vec<PersistedDeltaRow> = sqlx::query_as!(
        PersistedDeltaRow,
        "SELECT rd.request_id, rd.predecessor_request_id,
                ck.code AS \"change_kind!\", rd.ord,
                rd.message_id, m.message_sha256 AS \"message_sha256!\",
                ro.code AS \"role!\", m.byte_size AS \"byte_size!\", m.body AS \"body!\",
                rr.code AS \"removed_reason?\", rd.computed_at
           FROM requests.deltas rd
           JOIN messages.messages m ON m.id = rd.message_id
           JOIN requests.delta_change_kinds ck ON ck.id = rd.change_kind_id
           JOIN messages.roles ro ON ro.id = m.role_id
           LEFT JOIN requests.delta_removed_reasons rr ON rr.id = rd.removed_reason_id
          WHERE rd.request_id = $1
            AND rd.session_id = $2
          ORDER BY ck.code ASC, rd.ord ASC",
        request_id,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
