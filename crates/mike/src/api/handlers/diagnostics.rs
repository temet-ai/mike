use super::*;

pub(crate) async fn get_diagnostic_stats(
    State(state): State<AppState>,
    Query(params): Query<DiagnosticStatsParams>,
) -> Result<Json<DiagnosticStats>, StatusCode> {
    let Some(handle) = state.persist() else {
        let view = {
            let store = state.store.read().await;
            store.diagnostic_stats(chrono::Utc::now())
        };
        return Ok(Json(DiagnosticStats::from(&view)));
    };
    match handle
        .aggregate_diagnostic_stats(params.project.as_deref())
        .await
    {
        Ok(rows) => {
            let mut totals_by_category: BTreeMap<DiagnosticCategory, usize> = BTreeMap::new();
            for (cat_str, count) in rows.by_category {
                if let Ok(cat) =
                    serde_json::from_value::<DiagnosticCategory>(serde_json::Value::String(cat_str))
                {
                    let entry = totals_by_category.entry(cat).or_insert(0);
                    *entry = entry.saturating_add(count.max(0) as usize);
                }
            }
            let mut totals_by_analyzer: BTreeMap<String, usize> = BTreeMap::new();
            for (analyzer, count) in rows.by_analyzer {
                let entry = totals_by_analyzer.entry(analyzer).or_insert(0);
                *entry = entry.saturating_add(count.max(0) as usize);
            }
            let last_24h: usize = totals_by_category.values().sum();
            Ok(Json(DiagnosticStats {
                totals_by_category,
                totals_by_analyzer,
                last_24h,
            }))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "get_diagnostic_stats: DB query failed"
            );
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

pub(crate) async fn get_diagnostic_findings(
    State(state): State<AppState>,
    Query(params): Query<FindingsParams>,
) -> Response {
    let limit = params
        .limit
        .unwrap_or(FINDINGS_LIST_DEFAULT_LIMIT)
        .min(FINDINGS_LIST_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let filter = FindingsFilter {
        category: params.category.as_ref().map(|c| c.as_str()),
        analyzer: params.analyzer.as_deref(),
        session_id: params.session_id.as_deref(),
        project: params.project.as_deref(),
    };
    let want_total = params.total;

    let Some(handle) = state.persist() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let rows_result = handle
        .list_diagnostic_findings(&crate::store::persistence::rehydrate::FindingsQuery {
            offset,
            limit,
            category: filter.category,
            analyzer: filter.analyzer,
            session_id: filter.session_id,
            project: filter.project,
            with_total: want_total,
            order: params.order.unwrap_or_default(),
        })
        .await;
    let rows = match rows_result {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "get_diagnostic_findings: DB query failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let total_from_window = rows.first().map(|r| r.total_rows.max(0) as u64);
    let findings: Vec<FindingDto> = rows.iter().filter_map(finding_dto_from_row).collect();
    if !want_total {
        return Json(findings).into_response();
    }
    let total = resolve_findings_total(handle, total_from_window, &filter).await;
    Json(FindingsPage {
        findings,
        total,
        offset,
        limit,
    })
    .into_response()
}

struct FindingsFilter<'a> {
    category: Option<&'a str>,
    analyzer: Option<&'a str>,
    session_id: Option<&'a str>,
    project: Option<&'a str>,
}

async fn resolve_findings_total(
    handle: &crate::store::persistence::PersistHandle,
    total_from_window: Option<u64>,
    filter: &FindingsFilter<'_>,
) -> u64 {
    if let Some(t) = total_from_window {
        return t;
    }
    match handle
        .count_diagnostic_findings(
            filter.category,
            filter.analyzer,
            filter.session_id,
            filter.project,
        )
        .await
    {
        Ok(n) => n.max(0) as u64,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "get_diagnostic_findings: total fallback count failed; reporting 0"
            );
            0
        }
    }
}

pub(crate) fn finding_dto_from_row(
    row: &crate::store::persistence::rehydrate::DiagnosticFindingRow,
) -> Option<FindingDto> {
    let request_id = if row.request_id < 0 {
        return None;
    } else {
        row.request_id as u64
    };
    let category: DiagnosticCategory =
        serde_json::from_value(serde_json::Value::String(row.category.clone())).ok()?;
    let match_range: (usize, usize) = row
        .match_range
        .as_ref()
        .and_then(|v| {
            v.as_array().and_then(|arr| {
                if arr.len() == 2 {
                    let start = arr[0].as_u64().unwrap_or(0) as usize;
                    let end = arr[1].as_u64().unwrap_or(0) as usize;
                    Some((start, end))
                } else {
                    None
                }
            })
        })
        .unwrap_or((0, 0));
    let other_hits: usize = row.other_hits.as_array().map(|arr| arr.len()).unwrap_or(0);
    Some(FindingDto {
        request_id,
        session_id: row.session_id.clone(),
        category,
        analyzer: row.analyzer.clone(),
        advice: row.advice.clone(),
        matched_text: row.matched_text.clone(),
        match_range,
        other_hits,
        created_at: row.created_at.to_rfc3339(),
        request: FindingRequestRef {
            model: row.request_model.clone(),
            status: row.request_status.clone(),
            path: row.request_path.clone(),
            project: row
                .project_path
                .clone()
                .filter(|p| p != crate::store::persistence::SENTINEL_PROJECT_PATH),
            human_prompt: row.request_human_prompt.clone(),
            created_at: row.request_created_at.to_rfc3339(),
        },
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticPatternsResponse {
    pub(crate) patterns: Vec<crate::analysis::corpus::CorpusEntry>,
}

pub(crate) async fn get_diagnostic_patterns(
    State(state): State<AppState>,
) -> Json<DiagnosticPatternsResponse> {
    let registry = state.analyzer_registry().load();
    Json(DiagnosticPatternsResponse {
        patterns: crate::analysis::corpus::effective_entries(&registry),
    })
}
