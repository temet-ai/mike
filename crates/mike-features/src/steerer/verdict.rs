use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    Low,
    Medium,
    #[default]
    High,
}

pub const NEED_RUBRIC: &str = "How much this note is needed now. Weigh four things: what the operator asked across the whole thread, what this turn actually did, how much work is in flight (do not interrupt long multi-step work for a small gap), and how the operator has been reacting (short sharp corrections and repeated asks raise the grade of a real miss; they never justify repeating what the operator has just said themselves). high: the operator will have to ask again, or work is reported as done that is not, and nothing in the turn shows the session knows. medium: a real gap, but small next to the work done, or one the session is likely to meet on its next step. low: a formality, a preference, something an earlier note or the operator already said, or something the session has already answered. When fires=false return low.";

pub const NUDGE_WORDING: &str = "The note for the session. Empty when fires=false. One or two plain sentences: what the operator asked for and what you could not find in this turn, with room for it being done already. For example: 'You asked for the clearance value to be fixed and I do not see that in this turn. If it is done, say where; if not, it is still open.' Do not accuse, do not write 'instead' or 'state plainly', and give no ultimatum.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteererVerdict {
    pub fires: bool,
    pub confidence: f64,
    pub reason: String,
    pub nudge: String,
    #[serde(default)]
    pub need: Need,
    #[serde(default)]
    pub checklist: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChecklistItem {
    pub criterion: String,
    pub status: String,
    pub evidence: String,
}

pub fn clears_confidence_threshold(verdict: &SteererVerdict, threshold: f64) -> bool {
    verdict.confidence >= threshold
}

pub fn steerer_verdict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "fires": {
                "type": "boolean",
                "description": "Whether the steerer should fire an intervention for this turn"
            },
            "confidence": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Confidence score for the verdict (0.0–1.0)"
            },
            "reason": {
                "type": "string",
                "description": "Short reason explaining why the steerer is or is not firing"
            },
            "nudge": {
                "type": "string",
                "description": NUDGE_WORDING
            },
            "need": {
                "type": "string",
                "enum": ["low", "medium", "high"],
                "description": NEED_RUBRIC
            },
            "checklist": {
                "type": "array",
                "description": "Per-ask checklist: 5-15 checkable criteria decomposed from the accumulated ask, each evaluated item-by-item against the trajectory digest. Return an empty array when no accumulated ask / trajectory digest was supplied in this prompt.",
                "items": {
                    "type": "object",
                    "properties": {
                        "criterion": {
                            "type": "string",
                            "description": "One checkable criterion decomposed from the accumulated ask"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["done", "partial", "not_done"],
                            "description": "Whether the trajectory digest shows this criterion satisfied"
                        },
                        "evidence": {
                            "type": "string",
                            "description": "Evidence pointer (a 'req <id>' / 'tool <id>' citation) from the trajectory digest backing status; empty if none applies"
                        }
                    },
                    "required": ["criterion", "status", "evidence"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["fires", "confidence", "reason", "nudge", "need", "checklist"],
        "additionalProperties": false
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn steerer_verdict_deserializes() -> anyhow::Result<()> {
        let json = r#"{
            "fires": true,
            "confidence": 0.9,
            "reason": "missing test evidence",
            "nudge": "You claimed the tests pass, but no test command ran this turn - run the suite and show its output."
        }"#;
        let verdict: SteererVerdict = serde_json::from_str(json)?;
        assert!(verdict.fires);
        assert!((verdict.confidence - 0.9).abs() < 0.001);
        assert_eq!(verdict.reason, "missing test evidence");
        assert!(
            verdict.checklist.is_empty(),
            "a verdict blob predating the checklist field must default to empty, not fail to parse"
        );
        Ok(())
    }

    #[test]
    fn steerer_verdict_deserializes_with_checklist() -> anyhow::Result<()> {
        let json = r#"{
            "fires": false,
            "confidence": 0.4,
            "reason": "3 of 5 criteria delivered, work in flight",
            "nudge": "",
            "checklist": [
                {"criterion": "schema migration lands", "status": "done", "evidence": "req 101"},
                {"criterion": "consumer wiring", "status": "partial", "evidence": "tool toolu_02"},
                {"criterion": "tests pass", "status": "not_done", "evidence": ""}
            ]
        }"#;
        let verdict: SteererVerdict = serde_json::from_str(json)?;
        assert_eq!(verdict.checklist.len(), 3);
        assert_eq!(verdict.checklist[0].status, "done");
        assert_eq!(verdict.checklist[1].evidence, "tool toolu_02");
        assert_eq!(verdict.checklist[2].status, "not_done");
        Ok(())
    }

    fn verdict_with_confidence(confidence: f64) -> SteererVerdict {
        SteererVerdict {
            fires: true,
            confidence,
            reason: String::new(),
            nudge: String::new(),
            need: Need::High,
            checklist: Vec::new(),
        }
    }

    #[test]
    fn a_verdict_stored_before_the_need_grade_reads_as_high() -> anyhow::Result<()> {
        let stored = r#"{"fires": true, "confidence": 0.9, "reason": "r", "nudge": "n"}"#;
        let verdict: SteererVerdict = serde_json::from_str(stored)?;
        assert_eq!(verdict.need, Need::High);
        let graded: SteererVerdict = serde_json::from_str(
            r#"{"fires": true, "confidence": 0.9, "reason": "r", "nudge": "n", "need": "medium"}"#,
        )?;
        assert_eq!(graded.need, Need::Medium);
        assert!(Need::Low < Need::Medium && Need::Medium < Need::High);
        Ok(())
    }

    #[test]
    fn the_schema_asks_every_judge_for_the_need_grade_and_gives_the_wording_rule() {
        let schema = steerer_verdict_schema();
        assert_eq!(
            schema["properties"]["need"]["enum"],
            serde_json::json!(["low", "medium", "high"])
        );
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|v| v == "need"))
        );
        let wording = schema["properties"]["nudge"]["description"]
            .as_str()
            .unwrap_or_default();
        assert!(wording.contains("room for it being done already"));
        assert!(wording.contains("no ultimatum"));
    }

    #[test]
    fn clears_confidence_threshold_below_threshold_returns_false() {
        let verdict = verdict_with_confidence(0.6);
        assert!(!clears_confidence_threshold(&verdict, 0.7));
    }

    #[test]
    fn clears_confidence_threshold_equal_threshold_returns_true() {
        let verdict = verdict_with_confidence(0.7);
        assert!(clears_confidence_threshold(&verdict, 0.7));
    }

    #[test]
    fn clears_confidence_threshold_above_threshold_returns_true() {
        let verdict = verdict_with_confidence(0.8);
        assert!(clears_confidence_threshold(&verdict, 0.7));
    }

    #[test]
    fn clears_confidence_threshold_nan_confidence_returns_false() {
        let verdict = verdict_with_confidence(f64::NAN);
        assert!(!clears_confidence_threshold(&verdict, 0.7));
    }

    #[test]
    fn steerer_verdict_schema_is_valid_json() {
        let schema = steerer_verdict_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["fires"].is_object());
        assert!(schema["properties"]["nudge"].is_object());
        assert!(schema["properties"]["checklist"].is_object());
    }

    #[test]
    fn steerer_verdict_schema_requires_checklist_in_strict_mode() {
        let schema = steerer_verdict_schema();
        let Some(required) = schema["required"].as_array() else {
            panic!("schema[\"required\"] must be a JSON array");
        };
        assert!(required.iter().any(|v| v == "checklist"));

        let item_schema = &schema["properties"]["checklist"]["items"];
        let Some(item_required) = item_schema["required"].as_array() else {
            panic!("checklist item schema[\"required\"] must be a JSON array");
        };
        for field in ["criterion", "status", "evidence"] {
            assert!(
                item_required.iter().any(|v| v == field),
                "checklist item property {field} must be listed in its required array"
            );
        }
        assert_eq!(item_schema["additionalProperties"], false);
    }
}
