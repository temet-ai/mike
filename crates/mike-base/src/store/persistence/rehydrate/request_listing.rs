use sqlx::{PgPool, Postgres, QueryBuilder};

use super::pagination::{PageWindow, limit_to_i64};
use super::request_detail::count_session_requests;
use super::request_summary_row::{
    RequestSummaryRow, fetch_tool_names_for_keys, row_to_request_summary,
};
use crate::store::persistence::PersistError;
use crate::store::persistence::keyset::{KeysetCursor, probe_limit, split_page};
use crate::store::request_summary::RequestSummaryView;
use crate::types::SortOrder;

const LISTING_COLUMNS: &str = "SELECT s.session_uuid::text AS session_id, r.id, \
     rm.code AS method, r.path, r.model, r.is_streaming, r.message_count, \
     rs.code AS status, r.params, \
     r.human_prompt, rtt.code AS turn_type, rto.code AS turn_origin, r.subagent_persona, \
     r.created_at, r.completed_at, \
     rsp.prompt_sha256 AS prompt_sha256, pc.id AS composition_id, \
     NULL::bigint AS predecessor_request_id, NULL::bigint AS request_number, \
     r.agent_instance_id, NULL::bigint AS dispatch_request_id, \
     COALESCE(p.project_path, '(no project)') AS project_path ";

const LISTING_LOOKUP_JOINS: &str = "JOIN requests.methods rm ON rm.id = r.method_id \
     JOIN requests.statuses rs ON rs.id = r.status_id \
     JOIN requests.turn_types rtt ON rtt.id = r.turn_type_id \
     JOIN requests.turn_origins rto ON rto.id = r.turn_origin_id \
     JOIN sessions.sessions s ON s.id = r.session_id \
     LEFT JOIN overrides.overrides pc ON pc.id = rsp.override_id \
     LEFT JOIN projects.projects p ON p.id = s.project_id ";

pub async fn fetch_session_request_summaries_paginated(
    pool: &PgPool,
    session_db_id: i64,
    offset: usize,
    limit: usize,
    order: SortOrder,
) -> Result<(Vec<RequestSummaryView>, Option<usize>), PersistError> {
    let limit = limit_to_i64(limit);
    let offset = limit_to_i64(offset);
    let rows = match order {
        SortOrder::Asc => {
            sqlx::query_file_as!(
                RequestSummaryRow,
                "src/store/persistence/rehydrate/session_requests_asc.sql",
                session_db_id,
                limit,
                offset,
            )
            .fetch_all(pool)
            .await?
        }
        SortOrder::Desc => {
            sqlx::query_file_as!(
                RequestSummaryRow,
                "src/store/persistence/rehydrate/session_requests_desc.sql",
                session_db_id,
                limit,
                offset,
            )
            .fetch_all(pool)
            .await?
        }
    };

    let total_rows_seen = Some(count_session_requests(pool, session_db_id).await?);
    let out = summaries_with_tools(pool, rows).await?;
    Ok((out, total_rows_seen))
}

async fn summaries_with_tools(
    pool: &PgPool,
    rows: Vec<RequestSummaryRow>,
) -> Result<Vec<RequestSummaryView>, PersistError> {
    let keys: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let (mut declared_by_key, mut invoked_by_key) = fetch_tool_names_for_keys(pool, &keys).await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let key = row.id;
        let declared = declared_by_key.remove(&key).unwrap_or_default();
        let invoked = invoked_by_key.remove(&key).unwrap_or_default();
        out.push(row_to_request_summary(row, declared, invoked)?);
    }
    Ok(out)
}

fn push_window(query: &mut QueryBuilder<Postgres>, window: &PageWindow) {
    if let Some(from) = window.date_from {
        query.push(" AND r.created_at >= ").push_bind(from);
    }
    if let Some(to) = window.date_to {
        query.push(" AND r.created_at < ").push_bind(to);
    }
    if let Some(cursor) = window.cursor {
        query
            .push(" AND (r.created_at, r.id) ")
            .push(window.keyset_comparison())
            .push(" (")
            .push_bind(cursor.at)
            .push(", ")
            .push_bind(cursor.id)
            .push(")");
    }
    query
        .push(" ORDER BY r.created_at ")
        .push(window.order.sql())
        .push(", r.id ")
        .push(window.order.sql())
        .push(" LIMIT ")
        .push_bind(probe_limit(limit_to_i64(window.limit)))
        .push(" OFFSET ")
        .push_bind(window.effective_offset());
}

async fn fetch_requests_by_prompt_sha(
    pool: &PgPool,
    sha: &str,
    window: &PageWindow,
) -> Result<Vec<RequestSummaryRow>, PersistError> {
    let mut query = QueryBuilder::<Postgres>::new(LISTING_COLUMNS);
    query
        .push("FROM requests.requests r JOIN requests.system_prompts rsp ON rsp.request_id = r.id ")
        .push(LISTING_LOOKUP_JOINS)
        .push(
            "WHERE rsp.system_prompt_id IN \
             (SELECT sp.id FROM system_prompts.system_prompts sp WHERE sp.prompt_sha256 = ",
        )
        .push_bind(sha)
        .push(")");
    push_window(&mut query, window);
    let rows = query
        .build_query_as::<RequestSummaryRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

async fn fetch_requests_unfiltered(
    pool: &PgPool,
    window: &PageWindow,
) -> Result<Vec<RequestSummaryRow>, PersistError> {
    let mut query = QueryBuilder::<Postgres>::new(LISTING_COLUMNS);
    query
        .push("FROM requests.requests r LEFT JOIN requests.system_prompts rsp ON rsp.request_id = r.id ")
        .push(LISTING_LOOKUP_JOINS)
        .push("WHERE TRUE");
    push_window(&mut query, window);
    let rows = query
        .build_query_as::<RequestSummaryRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn list_requests_paginated(
    pool: &PgPool,
    window: PageWindow,
    prompt_sha256: Option<&str>,
) -> Result<(Vec<RequestSummaryView>, Option<KeysetCursor>), PersistError> {
    let probed: Vec<RequestSummaryRow> = match prompt_sha256 {
        Some(sha) => fetch_requests_by_prompt_sha(pool, sha, &window).await?,
        None => fetch_requests_unfiltered(pool, &window).await?,
    };
    let (rows, next_cursor) = split_page(probed, limit_to_i64(window.limit), |row| KeysetCursor {
        at: row.created_at,
        id: row.id,
    });
    let out = summaries_with_tools(pool, rows).await?;
    Ok((out, next_cursor))
}
