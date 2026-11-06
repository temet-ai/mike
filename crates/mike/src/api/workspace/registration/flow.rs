use serde::Serialize;

use crate::store::persistence::workspace::{FlowAdmissionEvidence, FlowAdmissionResult};

#[derive(Debug, Serialize)]
pub(super) struct FlowAdmissionDto {
    status: &'static str,
    evidence: Option<FlowEvidenceDto>,
}

#[derive(Debug, Serialize)]
struct FlowEvidenceDto {
    capture_version: i32,
    admitted_at: String,
    producer: String,
    hook_event_name: String,
    source: String,
}

impl From<FlowAdmissionEvidence> for FlowEvidenceDto {
    fn from(evidence: FlowAdmissionEvidence) -> Self {
        Self {
            capture_version: evidence.capture_version,
            admitted_at: evidence.admitted_at.to_rfc3339(),
            producer: evidence.producer,
            hook_event_name: evidence.hook_event_name,
            source: evidence.source,
        }
    }
}

impl From<FlowAdmissionResult> for FlowAdmissionDto {
    fn from(result: FlowAdmissionResult) -> Self {
        Self {
            status: if result.evidence.is_some() {
                "admitted"
            } else {
                "not_admitted"
            },
            evidence: result.evidence.map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_status_carries_evidence() -> anyhow::Result<()> {
        let admitted_at = "2026-09-10T12:00:00Z".parse()?;
        let dto: FlowAdmissionDto = FlowAdmissionResult {
            evidence: Some(FlowAdmissionEvidence {
                capture_version: 1,
                admitted_at,
                producer: "mike".into(),
                hook_event_name: "capture".into(),
                source: "capture".into(),
            }),
        }
        .into();
        let value = serde_json::to_value(dto)?;
        assert_eq!(value["status"], "admitted");
        assert_eq!(value["evidence"]["source"], "capture");
        assert_eq!(
            value["evidence"]["admitted_at"],
            "2026-09-10T12:00:00+00:00"
        );
        Ok(())
    }

    #[test]
    fn not_admitted_status_has_null_evidence() -> anyhow::Result<()> {
        let value = serde_json::to_value(FlowAdmissionDto::from(FlowAdmissionResult {
            evidence: None,
        }))?;
        assert_eq!(value["status"], "not_admitted");
        assert!(value["evidence"].is_null());
        Ok(())
    }
}
