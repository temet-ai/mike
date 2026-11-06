use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct FeatureToggleDto {
    pub(crate) key: &'static str,
    pub(crate) enabled: bool,
    pub(crate) scope: crate::feature_toggles::ToggleScope,
    pub(crate) applies: crate::feature_toggles::ToggleApplies,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeatureToggleErrorBody {
    pub error: String,
}

pub(crate) async fn list_features(State(state): State<AppState>) -> Json<serde_json::Value> {
    let overlay = state.feature_toggle_overlay.load();
    let features: Vec<FeatureToggleDto> = crate::feature_toggles::TOGGLE_META
        .iter()
        .map(|m| {
            let enabled =
                crate::feature_toggles::get_toggle_enabled(&overlay, m.key).unwrap_or(true);
            FeatureToggleDto {
                key: m.key,
                enabled,
                scope: m.scope,
                applies: m.applies,
            }
        })
        .collect();
    Json(serde_json::json!({ "features": features }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchFeatureBody {
    pub enabled: bool,
}

pub(crate) async fn patch_feature(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PatchFeatureBody>,
) -> Result<Json<FeatureToggleDto>, (StatusCode, Json<FeatureToggleErrorBody>)> {
    let Some(svc) = state.feature_toggle_service() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(FeatureToggleErrorBody {
                error: "session persistence is disabled; feature toggle mutation is not available"
                    .to_owned(),
            }),
        ));
    };

    let source = crate::api::change_source::change_source_from_headers(&headers);
    match svc.set_with_source(&key, body.enabled, &source).await {
        Ok((canonical_key, enabled, _change_ids)) => {
            let meta = crate::feature_toggles::TOGGLE_META
                .iter()
                .find(|m| m.key == canonical_key)
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(FeatureToggleErrorBody {
                            error: "internal: canonical key not found in TOGGLE_META".to_owned(),
                        }),
                    )
                })?;
            Ok(Json(FeatureToggleDto {
                key: canonical_key,
                enabled,
                scope: meta.scope,
                applies: meta.applies,
            }))
        }
        Err(crate::feature_toggles::service::FeatureToggleServiceError::NotFound(k)) => Err((
            StatusCode::NOT_FOUND,
            Json(FeatureToggleErrorBody {
                error: format!("unknown feature toggle: {k}"),
            }),
        )),
        Err(crate::feature_toggles::service::FeatureToggleServiceError::Persistence(e)) => {
            tracing::error!(error = %e, "failed to persist feature toggle update");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(FeatureToggleErrorBody {
                    error: format!("persistence error: {e}"),
                }),
            ))
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SettingErrorBody {
    pub error: String,
}

pub(crate) async fn list_settings(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_values: std::collections::HashMap<String, serde_json::Value> = match state
        .settings_service()
    {
        Some(svc) => match svc.fetch_all().await {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                tracing::warn!(error = %e, "failed to fetch settings from DB; falling back to compiled defaults");
                std::collections::HashMap::new()
            }
        },
        None => std::collections::HashMap::new(),
    };
    let launch_resolved = super::settings_01_launch_values::launch_resolved_values(&state);

    let settings: Vec<serde_json::Value> = crate::settings::SETTINGS_META
        .iter()
        .map(|m| {
            let value = db_values
                .get(m.key)
                .cloned()
                .or_else(|| launch_resolved.get(m.key).cloned())
                .unwrap_or_else(|| m.default.clone());
            let mut entry = serde_json::to_value(m).unwrap_or(serde_json::Value::Null);
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("value".to_owned(), value);
            }
            entry
        })
        .collect();

    let env_var_name = {
        let db_name = db_values
            .get("steerer.openrouter_api_key_env")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        db_name.unwrap_or_else(|| "MIKE_STEERER_OPENROUTER_API_KEY".to_owned())
    };
    let steerer_openrouter_configured_via_env =
        crate::config::reload_openrouter_key(state.config(), &env_var_name)
            .is_ok_and(|key| key.is_some());

    let value_of = |key: &str| {
        settings
            .iter()
            .find(|s| s["key"] == key)
            .map(|s| s["value"].clone())
            .unwrap_or_default()
    };
    let toggles = state.feature_toggle_overlay.load();
    let effective_for = |prefix: &str, enabled: bool, rule: &str| {
        let mut effective = compression_effective(
            (enabled, rule),
            value_of(&format!("{prefix}.mode"))
                .as_str()
                .unwrap_or("shadow"),
            value_of(&format!("{prefix}.forward_share"))
                .as_u64()
                .unwrap_or(0) as usize,
        );
        effective["session_arm"] = serde_json::Value::from(format!(
            "each request's manifest records its session's arm in the {prefix} stage's resolved_parameters.arm"
        ));
        effective
    };
    let file_read_compression_effective = effective_for(
        "file_read_compression",
        toggles.file_read_compression,
        "File-read compression",
    );
    let shell_output_compression_effective = effective_for(
        "shell_output_compression",
        toggles.shell_output_compression,
        "Shell-output compression",
    );

    Json(serde_json::json!({
        "settings": settings,
        "steerer_openrouter_configured_via_env": steerer_openrouter_configured_via_env,
        "file_read_compression_effective": file_read_compression_effective,
        "shell_output_compression_effective": shell_output_compression_effective,
    }))
}

fn compression_effective(
    (enabled, rule): (bool, &str),
    mode: &str,
    forward_share: usize,
) -> serde_json::Value {
    let mut effective = crate::compress::stage::effective_forwarding(mode, forward_share);
    effective["enabled"] = serde_json::Value::Bool(enabled);
    if !enabled {
        effective["state"] = serde_json::Value::from("off");
        effective["summary"] = serde_json::Value::from(format!(
            "{rule} is off: no session is sent shortened requests and nothing is recorded. Mode and share take effect for new sessions once it is turned on.",
        ));
    }
    effective
}

#[cfg(test)]
#[test]
fn compression_effective_reports_off_when_the_switch_is_off_whatever_the_share() {
    let off = compression_effective((false, "Shell-output compression"), "shadow", 20);
    assert_eq!(off["state"], "off");
    assert_eq!(off["enabled"], false);
    assert_eq!(off["forward_share"], 20);
    assert!(
        off["summary"]
            .as_str()
            .is_some_and(|s| s.starts_with("Shell-output compression is off"))
    );
    let on = compression_effective((true, "File-read compression"), "shadow", 20);
    assert_eq!(on["state"], "sampled_forwarding");
    assert_eq!(on["enabled"], true);
}

#[derive(Debug, Deserialize)]
pub(crate) struct PatchSettingBody {
    pub value: serde_json::Value,
}

async fn follow_live_bundle(
    svc: &std::sync::Arc<crate::settings::service::SettingsService>,
    key: &str,
    value: &serde_json::Value,
    previous_pointer: Option<i64>,
    source: &crate::change_log::ChangeSource,
) -> Vec<crate::store::persistence::eval_bundles::LiveModelSwap> {
    if let Some(previous) = previous_pointer {
        if let Some(next) = value.as_i64() {
            svc.retire_previous_live_bundle(previous, next, source)
                .await;
        }
        return Vec::new();
    }
    if crate::settings::live_bundle_follow::is_provider_key(key) {
        return svc.follow_provider_change(source).await;
    }
    let Some(model) = value.as_str() else {
        return Vec::new();
    };
    svc.follow_model_change(key, model, source)
        .await
        .into_iter()
        .collect()
}

fn insert_swaps(
    entry: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    swaps: Vec<crate::store::persistence::eval_bundles::LiveModelSwap>,
) {
    if crate::settings::live_bundle_follow::is_provider_key(key) {
        entry.insert(
            "live_bundle_swaps".to_owned(),
            serde_json::to_value(swaps).unwrap_or_default(),
        );
    } else if let Some(swap) = swaps.into_iter().next() {
        entry.insert(
            "live_bundle_swap".to_owned(),
            serde_json::to_value(swap).unwrap_or_default(),
        );
    }
}

pub(crate) async fn patch_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PatchSettingBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<SettingErrorBody>)> {
    let Some(svc) = state.settings_service() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SettingErrorBody {
                error: "session persistence is disabled; settings mutation is not available"
                    .to_owned(),
            }),
        ));
    };

    super::embedding_change::validate_disable(&key, &body.value, &headers).map_err(
        |(status, error)| {
            (
                status,
                Json(SettingErrorBody {
                    error: error.to_owned(),
                }),
            )
        },
    )?;
    let source = crate::api::change_source::change_source_from_headers(&headers);
    let previous_pointer = svc.live_pointer(&key);
    match svc.set_with_source(&key, body.value, &source).await {
        Ok((validated_value, _change_ids)) => {
            if key.starts_with("embedding.") {
                let embedding_service = state.embedding_service();
                let _ = embedding_service.apply_setting(&key, &validated_value);
            }
            let swaps =
                follow_live_bundle(svc, &key, &validated_value, previous_pointer, &source).await;
            let meta = crate::settings::SETTINGS_META
                .iter()
                .find(|m| m.key == key.as_str())
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SettingErrorBody {
                            error: "internal: key not found in SETTINGS_META after set succeeded"
                                .to_owned(),
                        }),
                    )
                })?;
            let mut entry = serde_json::to_value(meta).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SettingErrorBody {
                        error: format!("internal: failed to serialize setting meta: {e}"),
                    }),
                )
            })?;
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("value".to_owned(), validated_value);
                insert_swaps(obj, &key, swaps);
            }
            Ok(Json(entry))
        }
        Err(crate::settings::service::SettingsServiceError::NotFound(k)) => Err((
            StatusCode::NOT_FOUND,
            Json(SettingErrorBody {
                error: format!("unknown setting: {k}"),
            }),
        )),
        Err(crate::settings::service::SettingsServiceError::InvalidValue { key: k, reason }) => {
            Err((
                StatusCode::BAD_REQUEST,
                Json(SettingErrorBody {
                    error: format!("invalid value for setting '{k}': {reason}"),
                }),
            ))
        }
        Err(crate::settings::service::SettingsServiceError::Persistence(e)) => {
            tracing::error!(error = %e, "failed to persist settings update");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SettingErrorBody {
                    error: format!("persistence error: {e}"),
                }),
            ))
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct VerifySteererKeyResponse {
    pub(crate) env_var_name: String,
    pub(crate) configured: bool,
    pub(crate) valid: bool,
}

pub(crate) async fn probe_openrouter_key(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<(), String> {
    let url = format!("{base_url}/key");
    match client
        .get(&url)
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => Err(format!(
            "OpenRouter rejected the key (HTTP {})",
            resp.status().as_u16()
        )),
        Err(e) => Err(format!("request to OpenRouter failed: {e}")),
    }
}

pub(crate) fn mask_openrouter_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let n = chars.len();
    let tail: String = chars[n.saturating_sub(3)..].iter().collect();
    if n <= 15 {
        return format!("...{tail}");
    }
    let prefix: String = chars[..12].iter().collect();
    format!("{prefix}...{tail}")
}

async fn resolve_openrouter_env_var_name(state: &AppState) -> String {
    let db_name = match state.settings_service() {
        Some(svc) => match svc.fetch_all().await {
            Ok(rows) => rows
                .into_iter()
                .find(|(k, _)| k == "steerer.openrouter_api_key_env")
                .and_then(|(_, v)| v.as_str().map(str::to_owned)),
            Err(_) => None,
        },
        None => None,
    };
    db_name.unwrap_or_else(|| "MIKE_STEERER_OPENROUTER_API_KEY".to_owned())
}

async fn verify_and_apply_openrouter_key(state: &AppState, key: &str) -> bool {
    let base_url = state.config().steerer.openrouter_base_url.clone();
    match probe_openrouter_key(&state.client, &base_url, key).await {
        Ok(()) => {
            state.steerer_openrouter_key.store(std::sync::Arc::new(Some(
                secrecy::SecretString::new(key.to_owned().into()),
            )));
            state.model_catalogue().bust_cache().await;
            true
        }
        Err(reason) => {
            tracing::debug!(
                reason,
                "steerer openrouter key verification: upstream rejected key"
            );
            false
        }
    }
}

pub(crate) async fn verify_steerer_openrouter_key(
    State(state): State<AppState>,
) -> Json<VerifySteererKeyResponse> {
    let env_var_name = resolve_openrouter_env_var_name(&state).await;

    use secrecy::ExposeSecret;
    let key_value = crate::config::reload_openrouter_key(state.config(), &env_var_name)
        .unwrap_or_else(|_| {
            tracing::warn!("cannot reload credentials from selected configuration");
            None
        });
    let configured = key_value.is_some();

    let valid = match key_value.as_ref() {
        Some(key) => verify_and_apply_openrouter_key(&state, key.expose_secret()).await,
        None => false,
    };

    tracing::debug!(
        env_var_name = %env_var_name,
        configured = configured,
        valid = valid,
        "steerer openrouter key verification"
    );

    Json(VerifySteererKeyResponse {
        env_var_name,
        configured,
        valid,
    })
}

#[derive(Deserialize)]
pub(crate) struct PutOpenRouterKeyBody {
    pub(crate) key: String,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct PutOpenRouterKeyResponse {
    pub(crate) valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    pub(crate) persisted: bool,
}
