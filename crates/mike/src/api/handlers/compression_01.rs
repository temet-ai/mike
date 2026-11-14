use super::*;

fn unified_body_diff(original: &[u8], compressed: &[u8]) -> String {
    if original == compressed {
        return String::new();
    }
    let original = String::from_utf8_lossy(original);
    let compressed = String::from_utf8_lossy(compressed);
    similar::TextDiff::from_lines(original.as_ref(), compressed.as_ref())
        .unified_diff()
        .context_radius(usize::MAX)
        .header("original", "compressed")
        .to_string()
}

#[derive(Debug, Serialize)]
pub(crate) struct CompressEstimateResponse {
    pub(crate) request_id: u64,
    pub(crate) compressed: bool,
    pub(crate) original_body: String,
    pub(crate) sent_body: String,
    pub(crate) report: crate::compress::measure::SavingsReport,
    pub(crate) manifest: crate::wire::manifest::TransformationManifest,
    pub(crate) diff: String,
}

pub(crate) async fn compress_estimate(
    State(state): State<AppState>,
    Json(body): Json<CompressEstimateRequest>,
) -> Result<Json<CompressEstimateResponse>, StatusCode> {
    let id = body.request_id;
    let req = load_intercepted_request(&state, id).await?;
    let real_input_tokens = req.token_usage.as_ref().map(|u| u.input_tokens);

    let Some(raw) = req.request_body.as_ref() else {
        let report = crate::compress::measure::SavingsReport::compute(0, 0, real_input_tokens);
        let empty_sha = crate::wire::manifest::sha256_hex(b"");
        return Ok(Json(CompressEstimateResponse {
            request_id: id,
            compressed: false,
            original_body: String::new(),
            sent_body: String::new(),
            report,
            manifest: crate::wire::manifest::TransformationManifest {
                schema_version: crate::wire::manifest::MANIFEST_SCHEMA_VERSION,
                original_sha256: empty_sha.clone(),
                sent_sha256: empty_sha,
                stages: Vec::new(),
            },
            diff: String::new(),
        }));
    };

    let default_enabled_producers = vec!["Read".to_string()];
    let default_protected_paths: Vec<String> = Vec::new();
    let policy = crate::compress::file_reads::FileReadPolicy::new(
        crate::compress::file_reads::DEFAULT_MIN_RESULT_BYTES,
        (
            crate::compress::file_reads::DEFAULT_HEAD_LINES,
            crate::compress::file_reads::DEFAULT_TAIL_LINES,
        ),
        crate::compress::file_reads::DEFAULT_GRACE_TURNS,
        &default_enabled_producers,
        &default_protected_paths,
    );
    let reduction = crate::compress::file_reads::reduce_file_reads(raw.as_ref(), &policy);
    let report = crate::compress::measure::SavingsReport::compute(
        raw.len(),
        reduction.candidate.len(),
        real_input_tokens,
    );
    let compressed = reduction.stage.outcome == crate::wire::manifest::StageOutcome::Applied;
    let diff = unified_body_diff(raw.as_ref(), reduction.candidate.as_ref());
    let manifest = crate::wire::manifest::TransformationManifest {
        schema_version: crate::wire::manifest::MANIFEST_SCHEMA_VERSION,
        original_sha256: crate::wire::manifest::sha256_hex(raw.as_ref()),
        sent_sha256: crate::wire::manifest::sha256_hex(reduction.candidate.as_ref()),
        stages: vec![reduction.stage],
    };
    if let Err(e) = manifest.verify_chain() {
        tracing::warn!(request_id = id, error = %e, "compression preview manifest hash chain did not close");
    }

    Ok(Json(CompressEstimateResponse {
        request_id: id,
        compressed,
        original_body: String::from_utf8_lossy(raw.as_ref()).into_owned(),
        sent_body: String::from_utf8_lossy(reduction.candidate.as_ref()).into_owned(),
        report,
        manifest,
        diff,
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompressStatsQuery {
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CompressStatsResponse {
    pub(crate) requests_scanned: usize,
    pub(crate) requests_compressed: usize,
    pub(crate) total_original_tokens: u64,
    pub(crate) total_est_compressed_tokens: u64,
    pub(crate) total_saved_pct: f64,
    pub(crate) total_original_bytes: u64,
    pub(crate) total_compressed_bytes: u64,
}

pub(crate) async fn compress_stats(
    State(state): State<AppState>,
    Query(params): Query<CompressStatsQuery>,
) -> Result<Json<CompressStatsResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).clamp(1, 500);
    let cfg = state
        .config
        .sections
        .get::<crate::features_settings::CompressConfig>()
        .for_measurement();

    let snapshot: Vec<(Bytes, Option<u32>)> = {
        let store = state.store.read().await;
        store
            .recent(limit)
            .into_iter()
            .filter_map(|req| {
                req.request_body
                    .as_ref()
                    .map(|b| (b.clone(), req.token_usage.as_ref().map(|u| u.input_tokens)))
            })
            .collect()
    };

    let mut requests_scanned = 0usize;
    let mut requests_compressed = 0usize;
    let mut total_original_tokens = 0u64;
    let mut total_est_compressed_tokens = 0u64;
    let mut total_original_bytes = 0u64;
    let mut total_compressed_bytes = 0u64;

    for (raw, real_input_tokens) in &snapshot {
        requests_scanned += 1;
        let (original_len, compressed_len, did_compress) =
            match crate::compress::compress_body(raw.as_ref(), &cfg) {
                crate::compress::CompressOutcome::Unchanged => (raw.len(), raw.len(), false),
                crate::compress::CompressOutcome::Compressed {
                    original_len,
                    compressed_len,
                    ..
                } => (original_len, compressed_len, true),
            };
        if did_compress {
            requests_compressed += 1;
        }
        let report = crate::compress::measure::SavingsReport::compute(
            original_len,
            compressed_len,
            *real_input_tokens,
        );
        total_original_tokens = total_original_tokens.saturating_add(report.est_original_tokens);
        total_est_compressed_tokens =
            total_est_compressed_tokens.saturating_add(report.est_compressed_tokens);
        total_original_bytes = total_original_bytes.saturating_add(original_len as u64);
        total_compressed_bytes = total_compressed_bytes.saturating_add(compressed_len as u64);
    }

    let total_saved_pct = if total_original_tokens == 0 {
        0.0
    } else {
        let saved = total_original_tokens.saturating_sub(total_est_compressed_tokens);
        let raw = (saved as f64 / total_original_tokens as f64) * 100.0;
        (raw * 100.0).round() / 100.0
    };

    Ok(Json(CompressStatsResponse {
        requests_scanned,
        requests_compressed,
        total_original_tokens,
        total_est_compressed_tokens,
        total_saved_pct,
        total_original_bytes,
        total_compressed_bytes,
    }))
}

pub(crate) async fn load_intercepted_request(
    state: &AppState,
    id: u64,
) -> Result<InterceptedRequest, StatusCode> {
    let in_memory = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(id)).cloned()
    };
    match in_memory {
        Some(req) => Ok(req),
        None => match load_request_from_db(state, id).await? {
            Some(req) => Ok(req),
            None => Err(StatusCode::NOT_FOUND),
        },
    }
}

#[derive(Serialize)]
pub(crate) struct SteeredRequestDto {
    #[serde(rename = "sent_request_id")]
    pub(crate) request_id: i64,
    #[serde(rename = "sent_response_id")]
    pub(crate) response_id: Option<i64>,
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) steerer_name: String,
    pub(crate) steerer_display_name: String,
    #[serde(rename = "steerer_model")]
    pub(crate) model: Option<String>,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) verdict: String,
    pub(crate) confidence: Option<f64>,
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) anchor_text: Option<String>,
    pub(crate) payload_text: Option<String>,
    pub(crate) verdict_json: Option<serde_json::Value>,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) sent_at: Option<String>,
    pub(crate) processed_at: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) reverted_at: Option<String>,
    pub(crate) revert_reason: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompressEstimateRequest {
    pub(crate) request_id: u64,
}
