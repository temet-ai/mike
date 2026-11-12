use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct PromptCompositionImportRowResult {
    pub(crate) prompt_sha256: String,
    pub(crate) status: &'static str,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PromptCompositionImportResult {
    pub(crate) imported: u32,
    pub(crate) skipped: u32,
    pub(crate) errors: u32,
    pub(crate) rows: Vec<PromptCompositionImportRowResult>,
}

pub(crate) async fn import_prompt_compositions(
    State(state): State<AppState>,
    Json(body): Json<PromptCompositionExport>,
) -> Result<Json<PromptCompositionImportResult>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(composition_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; import is not available",
        ));
    };
    if body.version != EXPORT_VERSION_CURRENT {
        return Err(composition_error_response(
            StatusCode::BAD_REQUEST,
            &format!(
                "unsupported export version {}; expected {EXPORT_VERSION_CURRENT}",
                body.version
            ),
        ));
    }
    let mut rows: Vec<PromptCompositionImportRowResult> =
        Vec::with_capacity(body.compositions.len());
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;
    for entry in body.compositions {
        let row = import_one_composition(handle, entry).await;
        match row.status {
            "imported" => imported += 1,
            "skipped" => skipped += 1,
            _ => errors += 1,
        }
        rows.push(row);
    }
    if imported > 0 {
        refresh_composer_cache(&state).await;
        republish_bundles_after_composition(&state, "import").await;
    }
    Ok(Json(PromptCompositionImportResult {
        imported,
        skipped,
        errors,
        rows,
    }))
}

async fn import_one_composition(
    handle: &crate::store::persistence::PersistHandle,
    entry: PromptCompositionExportEntry,
) -> PromptCompositionImportRowResult {
    if parse_prompt_sha256_path(&entry.prompt_sha256).is_err() {
        return PromptCompositionImportRowResult {
            prompt_sha256: entry.prompt_sha256,
            status: "error",
            reason: Some("prompt_sha256 must be 64 lowercase hex characters".to_string()),
        };
    }
    if entry.label.is_empty() {
        return PromptCompositionImportRowResult {
            prompt_sha256: entry.prompt_sha256,
            status: "error",
            reason: Some("label must not be empty".to_string()),
        };
    }
    let mode = match crate::wire::composition::CompositionMode::from_str(&entry.mode) {
        Ok(m) => m,
        Err(_) => {
            return PromptCompositionImportRowResult {
                prompt_sha256: entry.prompt_sha256,
                status: "error",
                reason: Some(format!(
                    "invalid mode {:?} (must be observe|template|replace)",
                    entry.mode
                )),
            };
        }
    };
    let blocks_json = match validate_blocks_input(&entry.blocks) {
        Ok(v) => v,
        Err(err_tuple) => {
            return PromptCompositionImportRowResult {
                prompt_sha256: entry.prompt_sha256,
                status: "error",
                reason: Some(err_tuple.1.error.clone()),
            };
        }
    };
    match handle
        .create_composition(
            crate::store::persistence::compositions::NewCompositionParams {
                prompt_sha256: &entry.prompt_sha256,
                label: &entry.label,
                blocks: &blocks_json,
                mode: Some(mode),
                agent_name: entry.agent_name.as_deref(),
                agent_definition_path: entry.agent_definition_path.as_deref(),
            },
        )
        .await
    {
        Ok(_) => PromptCompositionImportRowResult {
            prompt_sha256: entry.prompt_sha256,
            status: "imported",
            reason: None,
        },
        Err(err) => import_error_row(entry.prompt_sha256, err),
    }
}

fn import_error_row(
    prompt_sha256: String,
    err: crate::store::persistence::PersistError,
) -> PromptCompositionImportRowResult {
    use crate::store::persistence::PersistError;
    let is_unique = matches!(
        &err,
        PersistError::Database(sqlx_err)
            if sqlx_err.as_database_error().is_some_and(|d| d.is_unique_violation())
    );
    if is_unique {
        PromptCompositionImportRowResult {
            prompt_sha256,
            status: "skipped",
            reason: Some("a composition is already registered for this prompt class".to_string()),
        }
    } else {
        tracing::warn!(error = %err, "import composition row failed");
        PromptCompositionImportRowResult {
            prompt_sha256,
            status: "error",
            reason: Some(err.to_string()),
        }
    }
}
