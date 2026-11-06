use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::blocks::UnknownShapeReason;

#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    pub id: i64,
    pub uuid: Uuid,
    pub prompt_sha256: String,
    pub label: String,
    pub blocks: Vec<Value>,
    pub mode: CompositionMode,
    pub lane: String,
    pub notes: Option<String>,
    pub agent_id: Option<i64>,
    pub agent_name: Option<String>,
    pub agent_definition_path: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionMode {
    Observe,
    Template,
    Replace,
}

impl CompositionMode {
    pub(crate) fn from_str(raw: &str) -> Result<Self, String> {
        match raw {
            "observe" => Ok(Self::Observe),
            "template" => Ok(Self::Template),
            "replace" => Ok(Self::Replace),
            other => Err(format!(
                "unrecognized composition mode {other:?}; expected observe|template|replace"
            )),
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Template => "template",
            Self::Replace => "replace",
        }
    }
}

#[derive(Debug)]
pub(crate) enum Decision {
    Passthrough {
        prompt_sha256: Option<String>,
    },
    Templated {
        prompt_sha256: String,
        composition_id: i64,
    },
    Replace {
        prompt_sha256: String,
    },
    UnknownShape {
        unknown_blocks: Value,
    },
}

pub(crate) struct DecisionInputs<'a> {
    pub(crate) enabled: bool,
    pub(crate) parse_result: Result<&'a str, &'a UnknownShapeReason>,
    pub(crate) lookup: Option<&'a Composition>,
}

pub(crate) fn decide(
    inputs: DecisionInputs<'_>,
    computed_prompt_sha256: Option<String>,
) -> Decision {
    if !inputs.enabled {
        return Decision::Passthrough {
            prompt_sha256: None,
        };
    }

    if let Err(reason) = inputs.parse_result {
        return Decision::UnknownShape {
            unknown_blocks: reason.to_json(),
        };
    }

    let composition = match inputs.lookup {
        Some(c) => c,
        None => {
            return Decision::Passthrough {
                prompt_sha256: computed_prompt_sha256,
            };
        }
    };

    let prompt_sha256 = composition.prompt_sha256.clone();
    let composition_id = composition.id;
    match composition.mode {
        CompositionMode::Observe => Decision::Passthrough {
            prompt_sha256: computed_prompt_sha256.or(Some(prompt_sha256)),
        },
        CompositionMode::Template => Decision::Templated {
            prompt_sha256,
            composition_id,
        },
        CompositionMode::Replace => Decision::Replace { prompt_sha256 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn comp(prompt_sha256: &str, mode: CompositionMode) -> Composition {
        Composition {
            id: 0,
            uuid: Uuid::nil(),
            prompt_sha256: prompt_sha256.to_string(),
            label: "test".to_string(),
            blocks: vec![
                json!({"type": "text", "text": "b0"}),
                json!({"type": "text", "text": "b1"}),
                json!({"type": "text", "text": "b2"}),
            ],
            mode,
            notes: None,
            agent_id: None,
            agent_name: None,
            agent_definition_path: None,
            lane: "main".to_string(),
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn disabled_returns_passthrough_without_prompt_sha256() {
        let inputs = DecisionInputs {
            enabled: false,
            parse_result: Ok("hello"),
            lookup: None,
        };
        match decide(inputs, Some("classid".to_string())) {
            Decision::Passthrough { prompt_sha256 } => assert!(prompt_sha256.is_none()),
            other => panic!("expected Passthrough, got {other:?}"),
        }
    }

    #[test]
    fn unknown_shape_returns_unknown_blocks() {
        let reason = UnknownShapeReason::StringForm;
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Err(&reason),
            lookup: None,
        };
        match decide(inputs, None) {
            Decision::UnknownShape { unknown_blocks } => {
                assert_eq!(unknown_blocks, json!({"reason": "string_form"}));
            }
            other => panic!("expected UnknownShape, got {other:?}"),
        }
    }

    #[test]
    fn unknown_class_returns_passthrough_with_prompt_sha256() {
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Ok("hello"),
            lookup: None,
        };
        match decide(inputs, Some("class-novel".to_string())) {
            Decision::Passthrough { prompt_sha256 } => {
                assert_eq!(prompt_sha256.as_deref(), Some("class-novel"))
            }
            other => panic!("expected Passthrough, got {other:?}"),
        }
    }

    #[test]
    fn observe_mode_prefers_own_computed_identity_over_composition_sha() {
        let composition = comp("c-class-canonical", CompositionMode::Observe);
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Ok("hello"),
            lookup: Some(&composition),
        };
        match decide(inputs, Some("own-content-sha".to_string())) {
            Decision::Passthrough { prompt_sha256 } => {
                assert_eq!(
                    prompt_sha256.as_deref(),
                    Some("own-content-sha"),
                    "observe mode never rewrites the body, so it must report this request's \
                     own content identity, not the class's registered composition sha"
                )
            }
            other => panic!("expected Passthrough, got {other:?}"),
        }
    }

    #[test]
    fn observe_mode_falls_back_to_composition_sha_when_identity_unavailable() {
        let composition = comp("c-class-canonical", CompositionMode::Observe);
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Ok("hello"),
            lookup: Some(&composition),
        };
        match decide(inputs, None) {
            Decision::Passthrough { prompt_sha256 } => {
                assert_eq!(prompt_sha256.as_deref(), Some("c-class-canonical"))
            }
            other => panic!("expected Passthrough, got {other:?}"),
        }
    }

    #[test]
    fn template_mode_returns_templated() {
        let composition = comp("c-tpl", CompositionMode::Template);
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Ok("hello"),
            lookup: Some(&composition),
        };
        match decide(inputs, Some("c-tpl".to_string())) {
            Decision::Templated {
                prompt_sha256,
                composition_id,
            } => {
                assert_eq!(prompt_sha256, "c-tpl");
                assert_eq!(composition_id, 0i64);
            }
            other => panic!("expected Templated, got {other:?}"),
        }
    }

    #[test]
    fn replace_mode_returns_replace() {
        let composition = comp("c-rep", CompositionMode::Replace);
        let inputs = DecisionInputs {
            enabled: true,
            parse_result: Ok("hello"),
            lookup: Some(&composition),
        };
        match decide(inputs, Some("c-rep".to_string())) {
            Decision::Replace { prompt_sha256 } => assert_eq!(prompt_sha256, "c-rep"),
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn composition_mode_from_str_round_trips_for_known_values() -> anyhow::Result<()> {
        for (raw, expected) in [
            ("observe", CompositionMode::Observe),
            ("template", CompositionMode::Template),
            ("replace", CompositionMode::Replace),
        ] {
            let parsed = CompositionMode::from_str(raw).map_err(|e| anyhow::anyhow!(e))?;
            assert_eq!(parsed, expected);
        }
        Ok(())
    }

    #[test]
    fn composition_mode_from_str_rejects_unknown() -> anyhow::Result<()> {
        match CompositionMode::from_str("nope") {
            Ok(m) => Err(anyhow::anyhow!("expected Err, got Ok({m:?})")),
            Err(err) => {
                assert!(err.contains("unrecognized"), "got: {err}");
                Ok(())
            }
        }
    }

    #[test]
    fn composition_mode_as_str_round_trips() -> anyhow::Result<()> {
        for mode in [
            CompositionMode::Observe,
            CompositionMode::Template,
            CompositionMode::Replace,
        ] {
            let raw = mode.as_str();
            let parsed = CompositionMode::from_str(raw).map_err(|e| anyhow::anyhow!(e))?;
            assert_eq!(
                parsed, mode,
                "as_str/from_str round trip broken for {raw:?}"
            );
        }
        Ok(())
    }
}
