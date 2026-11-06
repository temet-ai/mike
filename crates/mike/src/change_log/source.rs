use serde::{Deserialize, Serialize};

use super::build::BuildStamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChangeVia {
    Spa,
    Api,
    Startup,
    Promotion,
    Mike,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChangeSource {
    pub(crate) via: ChangeVia,
    pub(crate) actor: Option<String>,
    pub(crate) request_id: Option<String>,
    pub(crate) detail: serde_json::Value,
}

impl ChangeSource {
    pub(crate) fn new(via: ChangeVia) -> Self {
        Self {
            via,
            actor: None,
            request_id: None,
            detail: serde_json::json!({}),
        }
    }

    pub(crate) fn internal() -> Self {
        Self::new(ChangeVia::Mike)
    }

    pub(crate) fn with_detail(mut self, key: &str, value: serde_json::Value) -> Self {
        if let Some(map) = self.detail.as_object_mut() {
            map.insert(key.to_owned(), value);
        }
        self
    }

    pub(crate) fn to_setting_json(&self, build: &BuildStamp) -> String {
        serde_json::json!({
            "via": self.via,
            "actor": self.actor,
            "request_id": self.request_id,
            "build_sha": build.sha,
            "detail": self.detail,
        })
        .to_string()
    }
}
