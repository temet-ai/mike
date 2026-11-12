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

pub(super) fn setting_entry(
    m: &crate::settings::SettingMeta,
    view: &crate::settings::service::MachineView,
    launch_resolved: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let value = view
        .value(m.key)
        .cloned()
        .or_else(|| launch_resolved.get(m.key).cloned())
        .unwrap_or_else(|| m.default.clone());
    let mut entry = serde_json::to_value(m).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = entry.as_object_mut() {
        obj.insert("value".to_owned(), value);
        if m.scope == crate::settings::SettingScope::Machine {
            obj.insert("source".to_owned(), serde_json::json!(view.source(m.key)));
            obj.insert(
                "shared_value".to_owned(),
                json_or_null(view.shared.get(m.key)),
            );
        }
    }
    entry
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

#[cfg(test)]
mod setting_entry_tests {
    use super::setting_entry;
    use crate::settings::service::MachineView;

    fn meta(key: &str) -> anyhow::Result<&'static crate::settings::SettingMeta> {
        crate::settings::SETTINGS_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| anyhow::anyhow!("{key} is not in SETTINGS_META"))
    }

    fn view(
        own: &[(&str, serde_json::Value)],
        shared: &[(&str, serde_json::Value)],
    ) -> MachineView {
        let to_map = |pairs: &[(&str, serde_json::Value)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect()
        };
        MachineView {
            machine_uuid: Some(uuid::Uuid::from_u128(1)),
            machine_name: Some("omit".to_owned()),
            own: to_map(own),
            shared: to_map(shared),
        }
    }

    #[test]
    fn setting_entry_machine_key_reports_own_value_and_the_shared_one() -> anyhow::Result<()> {
        let v = view(
            &[("launch_backend", serde_json::json!("tmux"))],
            &[("launch_backend", serde_json::json!("herdr"))],
        );
        let entry = setting_entry(meta("launch_backend")?, &v, &serde_json::Map::new());
        assert_eq!(entry["scope"], "machine");
        assert_eq!(entry["value"], "tmux");
        assert_eq!(entry["source"], "own");
        assert_eq!(entry["shared_value"], "herdr");
        Ok(())
    }

    #[test]
    fn setting_entry_machine_key_without_its_own_value_is_the_shared_default() -> anyhow::Result<()>
    {
        let v = view(&[], &[("launch_backend", serde_json::json!("herdr"))]);
        let entry = setting_entry(meta("launch_backend")?, &v, &serde_json::Map::new());
        assert_eq!(entry["value"], "herdr");
        assert_eq!(entry["source"], "shared_default");
        let bare = setting_entry(
            meta("launch_backend")?,
            &view(&[], &[]),
            &serde_json::Map::new(),
        );
        assert_eq!(bare["source"], "compiled_default");
        assert_eq!(bare["shared_value"], serde_json::Value::Null);
        Ok(())
    }

    #[test]
    fn setting_entry_shared_key_carries_no_source() -> anyhow::Result<()> {
        let v = view(&[], &[("launch_max_surfaces", serde_json::json!(4))]);
        let entry = setting_entry(meta("launch_max_surfaces")?, &v, &serde_json::Map::new());
        assert_eq!(entry["value"], 4);
        assert!(entry.get("source").is_none(), "{entry}");
        Ok(())
    }
}
