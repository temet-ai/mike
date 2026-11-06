use bytes::Bytes;
use serde_json::Value;

use super::catalogue;

pub(crate) fn declared_tool_values() -> Vec<Value> {
    catalogue()
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": (tool.input_schema)(),
            })
        })
        .collect()
}

pub(crate) enum DeclareOutcome {
    Unchanged,
    Declared {
        new_body: Bytes,
        declared: Vec<&'static str>,
    },
}

pub(crate) fn declare_tools(raw: &[u8]) -> DeclareOutcome {
    let declared = declared_tool_values();
    if declared.is_empty() {
        return DeclareOutcome::Unchanged;
    }

    let Ok(mut value) = serde_json::from_slice::<Value>(raw) else {
        return DeclareOutcome::Unchanged;
    };
    let Some(obj) = value.as_object_mut() else {
        return DeclareOutcome::Unchanged;
    };
    let Some(Value::Array(tools)) = obj.get_mut("tools") else {
        return DeclareOutcome::Unchanged;
    };
    if tools.is_empty() {
        return DeclareOutcome::Unchanged;
    }

    let existing: Vec<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let mut added: Vec<&'static str> = Vec::new();
    for (tool, rendered) in catalogue().iter().zip(declared) {
        if existing.iter().any(|n| n == tool.name) {
            continue;
        }
        tools.push(rendered);
        added.push(tool.name);
    }
    if added.is_empty() {
        return DeclareOutcome::Unchanged;
    }

    let spliced = crate::filter::content_blocks::splice_top_level_keys(raw, &value, &["tools"])
        .or_else(|| serde_json::to_vec(&value).ok());
    let Some(new_bytes) = spliced else {
        return DeclareOutcome::Unchanged;
    };
    DeclareOutcome::Declared {
        new_body: Bytes::from(new_bytes),
        declared: added,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures; hand-built JSON and fixture rows are expected to parse"
)]
mod tests {
    use super::*;
    use crate::proxy_tools::roster;

    fn declared_names(raw: &[u8]) -> Vec<String> {
        let v: Value = serde_json::from_slice(raw).expect("parse");
        v["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn appends_the_catalogue_after_the_client_tools() {
        let raw = br#"{"model":"m","messages":[],"system":"s","tools":[{"name":"Read"}]}"#;
        let DeclareOutcome::Declared { new_body, declared } = declare_tools(raw) else {
            panic!("expected a declaration");
        };
        assert_eq!(declared, vec![roster::TOOL_NAME]);
        assert_eq!(declared_names(&new_body), vec!["Read", roster::TOOL_NAME]);
    }

    #[test]
    fn every_other_top_level_key_stays_byte_identical() {
        let raw = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":[{"type":"text","text":"S"}],"tools":[{"name":"Read"}],"max_tokens":16}"#;
        let DeclareOutcome::Declared { new_body, .. } = declare_tools(raw) else {
            panic!("expected a declaration");
        };
        let before = String::from_utf8(raw.to_vec()).expect("utf8");
        let after = String::from_utf8(new_body.to_vec()).expect("utf8");
        let prefix = r#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":[{"type":"text","text":"S"}],"tools":["#;
        assert!(after.starts_with(prefix), "prefix churned: {after}");
        assert!(
            after.ends_with(r#"],"max_tokens":16}"#),
            "suffix churned: {after}"
        );
        assert!(before.len() < after.len());
    }

    #[test]
    fn a_second_pass_declares_nothing_further() {
        let raw = br#"{"messages":[],"tools":[{"name":"Read"}]}"#;
        let DeclareOutcome::Declared { new_body, .. } = declare_tools(raw) else {
            panic!("expected a declaration");
        };
        assert!(
            matches!(declare_tools(&new_body), DeclareOutcome::Unchanged),
            "re-declaring an already-declared tool would duplicate it on the wire"
        );
    }

    #[test]
    fn a_body_without_a_tools_key_is_left_untouched() {
        let raw = br#"{"model":"m","messages":[]}"#;
        assert!(matches!(declare_tools(raw), DeclareOutcome::Unchanged));
    }

    #[test]
    fn a_request_that_offers_the_model_no_tools_is_left_untouched() {
        let raw =
            br#"{"model":"m","messages":[{"role":"user","content":"Write the title"}],"tools":[]}"#;
        assert!(
            matches!(declare_tools(raw), DeclareOutcome::Unchanged),
            "a title or web-page request asks for text only; a lone declared tool gets called"
        );
    }

    #[test]
    fn a_non_array_tools_field_is_left_untouched() {
        let raw = br#"{"messages":[],"tools":"not-an-array"}"#;
        assert!(matches!(declare_tools(raw), DeclareOutcome::Unchanged));
    }

    #[test]
    fn a_body_that_is_not_json_is_left_untouched() {
        assert!(matches!(
            declare_tools(b"not json at all"),
            DeclareOutcome::Unchanged
        ));
    }

    #[test]
    fn the_declared_body_reconstructs_through_the_envelope_patch() -> anyhow::Result<()> {
        let raw = br#"{"model":"m","messages":[{"role":"user","content":"hi"}],"system":"S","tools":[{"name":"Read"}]}"#;
        let DeclareOutcome::Declared { new_body, .. } = declare_tools(raw) else {
            panic!("expected a declaration");
        };
        let patch = crate::store::persistence::messages::compute_envelope_patch(raw, &new_body)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .ok_or_else(|| anyhow::anyhow!("expected an envelope patch for the tools append"))?;
        let mut rebuilt = raw[..patch.start as usize].to_vec();
        rebuilt.extend_from_slice(&patch.bytes);
        rebuilt.extend_from_slice(&raw[patch.end as usize..]);
        assert_eq!(
            rebuilt,
            new_body.to_vec(),
            "request_sent must reconstruct the tools the proxy declared"
        );
        Ok(())
    }
}
