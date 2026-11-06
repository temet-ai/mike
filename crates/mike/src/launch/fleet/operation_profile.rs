use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::definition::FleetDefinition;
use super::operation::Execution;

pub const PROFILE_PATH: &str = ".claude/fleet/operations.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    schema_version: u32,
    execution: Execution,
}

pub fn apply_bytes(bytes: &[u8], definition: &mut Value) -> Result<(), String> {
    let profile: Profile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if profile.schema_version != 1 {
        return Err("operation profile schema_version must be 1".into());
    }
    let fields = definition
        .as_object_mut()
        .ok_or("fleet definition must be an object")?;
    if fields.get("execution") == Some(&Value::Null) {
        fields.insert("execution".into(), json!({}));
        return Ok(());
    }
    let mut execution = serde_json::to_value(profile.execution).map_err(|e| e.to_string())?;
    if let Some(overrides) = fields.get("execution") {
        let overrides = overrides.as_object().ok_or("execution must be an object")?;
        for (key, value) in overrides {
            if key == "operations" {
                let operations = value.as_object().ok_or("operations must be an object")?;
                for (name, operation) in operations {
                    if operation.is_null() {
                        execution["operations"]
                            .as_object_mut()
                            .map(|m| m.remove(name));
                    } else {
                        execution["operations"][name] = operation.clone();
                    }
                }
            } else {
                execution[key] = value.clone();
            }
        }
    }
    let parsed = FleetDefinition::from_value(&json!({"execution": execution}))?;
    let problems = super::operation::validate(&parsed);
    if !problems.is_empty() {
        return Err(format!("invalid operation profile: {problems:?}"));
    }
    fields.insert("execution".into(), execution);
    Ok(())
}

pub async fn apply(project: &str, definition: &mut Value) -> Result<(), String> {
    let path = Path::new(project).join(PROFILE_PATH);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let root = tokio::fs::canonicalize(project)
        .await
        .map_err(|e| e.to_string())?;
    let resolved = tokio::fs::canonicalize(&path)
        .await
        .map_err(|e| e.to_string())?;
    if !resolved.starts_with(root) {
        return Err("operation profile must resolve inside its project".into());
    }
    apply_bytes(&bytes, definition).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Vec<u8> {
        json!({"schema_version": 1, "execution": {"cargo": false, "operations": {
            "types": {"command": "npm run typecheck", "purpose": "types", "resources": ["ts-cache"]}
        }}})
        .to_string()
        .into_bytes()
    }

    #[test]
    fn operation_profile_defaults_and_whole_operation_overrides() -> anyhow::Result<()> {
        let mut definition = json!({});
        apply_bytes(&profile(), &mut definition).map_err(anyhow::Error::msg)?;
        assert_eq!(
            definition["execution"]["operations"]["types"]["resources"],
            json!(["ts-cache"])
        );
        let mut override_def = json!({"execution":{"cargo":true,"operations":{"types":{
            "command":"python3 -m unittest", "purpose":"tests"
        }}}});
        apply_bytes(&profile(), &mut override_def).map_err(anyhow::Error::msg)?;
        assert_eq!(override_def["execution"]["cargo"], true);
        assert!(
            override_def["execution"]["operations"]["types"]
                .get("resources")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn operation_profile_opt_out_and_invalid_contracts() -> anyhow::Result<()> {
        let mut definition = json!({"execution":{"operations":{"types":null}}});
        apply_bytes(&profile(), &mut definition).map_err(anyhow::Error::msg)?;
        assert_eq!(definition["execution"]["operations"], json!({}));
        let mut disabled = json!({"execution":null});
        apply_bytes(&profile(), &mut disabled).map_err(anyhow::Error::msg)?;
        assert_eq!(disabled["execution"], json!({}));
        assert!(apply_bytes(b"{}", &mut json!({})).is_err());
        assert!(
            apply_bytes(
                &profile(),
                &mut json!({"execution":{"operations":{"types":{"command":"x"}}}})
            )
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn operation_profile_missing_is_noop_and_malformed_is_error() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_string_lossy();
        let mut definition = json!({});
        apply(&root, &mut definition)
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(definition, json!({}));
        std::fs::create_dir_all(dir.path().join(".claude/fleet"))?;
        std::fs::write(dir.path().join(PROFILE_PATH), b"bad-json")?;
        assert!(apply(&root, &mut definition).await.is_err());
        Ok(())
    }
}
