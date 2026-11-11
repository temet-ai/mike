use super::SettingErrorBody;
use super::*;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MachineQuery {
    pub machine: Option<uuid::Uuid>,
}

pub(super) async fn machine_view_or_empty(
    state: &AppState,
    machine: Option<uuid::Uuid>,
) -> crate::settings::service::MachineView {
    let Some(svc) = state.settings_service() else {
        return Default::default();
    };
    svc.machine_view(machine).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to fetch settings from DB; falling back to compiled defaults");
        Default::default()
    })
}

pub(super) fn json_or_null(value: Option<&serde_json::Value>) -> serde_json::Value {
    value.cloned().unwrap_or(serde_json::Value::Null)
}

pub(super) type SettingError = (StatusCode, Json<SettingErrorBody>);

pub(super) fn setting_error(status: StatusCode, error: String) -> SettingError {
    (status, Json(SettingErrorBody { error }))
}

pub(super) fn service_error(e: crate::settings::service::SettingsServiceError) -> SettingError {
    use crate::settings::service::SettingsServiceError as E;
    match e {
        E::NotFound(k) => setting_error(StatusCode::NOT_FOUND, format!("unknown setting: {k}")),
        E::InvalidValue { key: k, reason } => setting_error(
            StatusCode::BAD_REQUEST,
            format!("invalid value for setting '{k}': {reason}"),
        ),
        E::Persistence(e) => {
            tracing::error!(error = %e, "failed to persist settings update");
            setting_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            )
        }
    }
}

pub(crate) async fn clear_machine_setting(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<MachineQuery>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, SettingError> {
    let svc = state.settings_service().ok_or_else(|| {
        setting_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; settings mutation is not available".to_owned(),
        )
    })?;
    let source = crate::api::change_source::change_source_from_headers(&headers);
    let (machine, cleared) = svc
        .clear_machine_value(&key, query.machine, &source)
        .await
        .map_err(service_error)?;
    let view = svc
        .machine_view(Some(machine))
        .await
        .map_err(service_error)?;
    Ok(Json(serde_json::json!({
        "key": key,
        "machine": { "uuid": machine, "name": view.machine_name },
        "cleared": cleared,
        "source": view.source(&key),
        "value": json_or_null(view.value(&key)),
    })))
}
