use std::collections::HashMap;

use sqlx::PgConnection;

use crate::store::models::{InterceptedRequest, ThreadKind, ThreadLink, is_fallback};
use crate::store::persistence::PersistError;
use crate::store::persistence::agent_instance_capture::is_subagent_origin;

use super::canonicalize::CanonicalMessage;
use super::chain_bridge::fetch_chain_bridge;
use super::delta::{DeltaRow, compute_delta, is_reliable_sidecar_origin};
use super::reads::{
    NEW_THREAD_REQUESTS, RECENT_REQUESTS, fetch_predecessor_primary, fetch_session_known_messages,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredecessorRule {
    ThreadLink,
    ThreadStart,
    ThreadRestart,
    LaneFallback,
    None,
}

const RESTART_MIN_MESSAGES: usize = 3;

impl PredecessorRule {
    pub fn code(self) -> &'static str {
        match self {
            Self::ThreadLink => "thread_link",
            Self::ThreadStart => "thread_start",
            Self::ThreadRestart => "thread_restart",
            Self::LaneFallback => "lane_fallback",
            Self::None => "none",
        }
    }
}

pub struct PredecessorDelta {
    pub predecessor_request_id: Option<i64>,
    pub rows: Vec<DeltaRow>,
    pub carry_forward: HashMap<String, i64>,
    pub rule: PredecessorRule,
}

impl PredecessorDelta {
    fn fresh(
        canonical: &[CanonicalMessage],
        known: HashMap<String, i64>,
        rule: PredecessorRule,
    ) -> Self {
        Self {
            predecessor_request_id: None,
            rows: compute_delta(canonical, None),
            carry_forward: known,
            rule,
        }
    }

    fn continuation(
        canonical: &[CanonicalMessage],
        predecessor: i64,
        known: HashMap<String, i64>,
        rule: PredecessorRule,
    ) -> Self {
        Self {
            predecessor_request_id: Some(predecessor),
            rows: compute_delta(canonical, None),
            carry_forward: known,
            rule,
        }
    }

    fn against(
        canonical: &[CanonicalMessage],
        predecessor: i64,
        bridge: Vec<(i32, String, i64)>,
        rule: PredecessorRule,
    ) -> Self {
        let carry_forward: HashMap<String, i64> = bridge
            .iter()
            .map(|(_, sha, message_id)| (sha.clone(), *message_id))
            .collect();
        let pred_shas: Vec<(i32, String)> =
            bridge.into_iter().map(|(ord, sha, _)| (ord, sha)).collect();
        Self {
            predecessor_request_id: Some(predecessor),
            rows: compute_delta(canonical, Some(&pred_shas)),
            carry_forward,
            rule,
        }
    }

    fn reusing(mut self, known: HashMap<String, i64>) -> Self {
        for (sha, message_id) in known {
            self.carry_forward.entry(sha).or_insert(message_id);
        }
        self
    }
}

pub async fn resolve_predecessor_and_delta(
    conn: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    canonical: &[CanonicalMessage],
    agent_instance_id: Option<i64>,
) -> Result<PredecessorDelta, PersistError> {
    if is_reliable_sidecar_origin(&req.turn_origin) {
        let known = fetch_session_known_messages(
            conn,
            session_db_id,
            req.created_at,
            canonical,
            RECENT_REQUESTS,
        )
        .await?;
        return Ok(PredecessorDelta::fresh(
            canonical,
            known,
            PredecessorRule::None,
        ));
    }
    let Some(link) = req.thread.as_ref() else {
        return lane_predecessor_and_delta(conn, req, session_db_id, canonical, agent_instance_id)
            .await;
    };
    if link.kind == ThreadKind::Create {
        return start_or_restart(conn, req, session_db_id, canonical, agent_instance_id).await;
    }
    let known = fetch_session_known_messages(
        conn,
        session_db_id,
        req.created_at,
        canonical,
        RECENT_REQUESTS,
    )
    .await?;
    if link.continues()
        && let Some(linked) = linked_predecessor(conn, link).await?
    {
        return Ok(PredecessorDelta::continuation(
            canonical,
            linked,
            known,
            PredecessorRule::ThreadLink,
        ));
    }
    Ok(
        match lane_predecessor(conn, req, session_db_id, agent_instance_id).await? {
            Some(guessed) => PredecessorDelta::continuation(
                canonical,
                guessed,
                known,
                PredecessorRule::LaneFallback,
            ),
            None => PredecessorDelta::fresh(canonical, known, PredecessorRule::None),
        },
    )
}

async fn start_or_restart(
    conn: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    canonical: &[CanonicalMessage],
    agent_instance_id: Option<i64>,
) -> Result<PredecessorDelta, PersistError> {
    let earlier = if canonical.len() >= RESTART_MIN_MESSAGES {
        lane_predecessor(conn, req, session_db_id, agent_instance_id).await?
    } else {
        None
    };
    let Some(predecessor) = earlier else {
        let known = fetch_session_known_messages(
            conn,
            session_db_id,
            req.created_at,
            canonical,
            NEW_THREAD_REQUESTS,
        )
        .await?;
        return Ok(PredecessorDelta::fresh(
            canonical,
            known,
            PredecessorRule::ThreadStart,
        ));
    };
    let chain = fetch_chain_bridge(conn, predecessor).await?;
    let mut restart = PredecessorDelta::against(
        canonical,
        predecessor,
        chain,
        PredecessorRule::ThreadRestart,
    );
    restart.rows.retain(|row| row.change_kind == "added");
    let known = fetch_session_known_messages(
        conn,
        session_db_id,
        req.created_at,
        canonical,
        NEW_THREAD_REQUESTS,
    )
    .await?;
    Ok(restart.reusing(known))
}

async fn linked_predecessor(
    conn: &mut PgConnection,
    link: &ThreadLink,
) -> Result<Option<i64>, PersistError> {
    if let Some(in_memory) = link.predecessor.filter(|id| !is_fallback(id.as_u64())) {
        return Ok(i64::try_from(in_memory.as_u64()).ok());
    }
    let Some(previous_message_id) = link.previous_message_id.as_deref() else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar!(
        "SELECT rr.request_id
           FROM responses.responses rr
          WHERE rr.upstream_message_id = $1
          ORDER BY rr.request_id DESC
          LIMIT 1",
        previous_message_id,
    )
    .fetch_optional(&mut *conn)
    .await?)
}

async fn lane_predecessor(
    conn: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    agent_instance_id: Option<i64>,
) -> Result<Option<i64>, PersistError> {
    if is_subagent_origin(&req.turn_origin) && agent_instance_id.is_none() {
        return Ok(None);
    }
    Ok(
        fetch_predecessor_primary(conn, session_db_id, req.created_at, agent_instance_id)
            .await?
            .map(|(id, _)| id),
    )
}

async fn lane_predecessor_and_delta(
    conn: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    canonical: &[CanonicalMessage],
    agent_instance_id: Option<i64>,
) -> Result<PredecessorDelta, PersistError> {
    let Some(predecessor) = lane_predecessor(conn, req, session_db_id, agent_instance_id).await?
    else {
        let known = fetch_session_known_messages(
            conn,
            session_db_id,
            req.created_at,
            canonical,
            RECENT_REQUESTS,
        )
        .await?;
        return Ok(PredecessorDelta::fresh(
            canonical,
            known,
            PredecessorRule::None,
        ));
    };
    let bridge = fetch_chain_bridge(conn, predecessor).await?;
    let known = fetch_session_known_messages(
        conn,
        session_db_id,
        req.created_at,
        canonical,
        RECENT_REQUESTS,
    )
    .await?;
    Ok(PredecessorDelta::against(
        canonical,
        predecessor,
        bridge,
        PredecessorRule::LaneFallback,
    )
    .reusing(known))
}
