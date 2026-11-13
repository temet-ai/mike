use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{FleetDefinition, report::VerdictRequest};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regression {
    pub task: String,
    pub criteria: Vec<String>,
    pub reason: String,
    pub failing_tests: Vec<String>,
    pub evidence: Vec<String>,
}

pub fn validate(def: &FleetDefinition, req: &VerdictRequest) -> Result<(), String> {
    for regression in &req.regressions {
        let task = def.tasks.get(&regression.task).ok_or_else(|| {
            format!(
                "regression target {} is not a task of this fleet",
                regression.task
            )
        })?;
        if regression.task == req.task {
            return Err(
                "cross-task regression must name another task; fail this task directly".into(),
            );
        }
        if regression.criteria.is_empty()
            || regression
                .criteria
                .iter()
                .any(|id| !task.criteria.contains(id) || !def.criteria.contains_key(id))
        {
            return Err(format!(
                "regression criteria must belong to target task {}",
                regression.task
            ));
        }
        if regression.reason.trim().is_empty()
            || !nonempty(&regression.failing_tests)
            || !nonempty(&regression.evidence)
        {
            return Err(
                "regression requires reason, failing_tests and evidence with nonempty entries"
                    .into(),
            );
        }
    }
    Ok(())
}

fn nonempty(values: &[String]) -> bool {
    !values.is_empty() && values.iter().all(|value| !value.trim().is_empty())
}

pub fn data(req: &VerdictRequest, regression: &Regression) -> Value {
    json!({
        "obligation_id": format!("regression-{}", Uuid::new_v4()),
        "task": regression.task,
        "criteria": regression.criteria,
        "outcome": "failed",
        "required": true,
        "source_task": req.task,
        "report_note": req.report_note,
        "reason": regression.reason,
        "failing_tests": regression.failing_tests,
        "evidence": regression.evidence,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn request() -> VerdictRequest {
        VerdictRequest {
            task: "T1".into(),
            regressions: vec![Regression {
                task: "T2".into(),
                criteria: vec!["C2".into()],
                reason: "footer hint disappeared".into(),
                failing_tests: vec!["status_hint_snapshot".into()],
                evidence: vec!["results/snapshot.diff".into()],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn cross_task_regression_requires_known_scope_and_evidence() -> anyhow::Result<()> {
        let def: FleetDefinition = serde_json::from_value(json!({
            "criteria": {"C1": {"text": "one"}, "C2": {"text": "two"}},
            "tasks": {"T1": {"criteria": ["C1"]}, "T2": {"criteria": ["C2"]}}
        }))?;
        let req = request();
        assert!(validate(&def, &req).is_ok());
        let mut invalid = req.clone();
        invalid.regressions[0].task = "T9".into();
        assert!(validate(&def, &invalid).is_err());
        invalid = req.clone();
        invalid.regressions[0].criteria = vec!["C1".into()];
        assert!(validate(&def, &invalid).is_err());
        invalid = req.clone();
        invalid.regressions[0].evidence.clear();
        assert!(validate(&def, &invalid).is_err());
        let data = data(&req, &req.regressions[0]);
        assert_eq!(data["task"], "T2");
        assert_eq!(data["outcome"], "failed");
        assert_eq!(data["required"], true);
        Ok(())
    }
}

#[cfg(test)]
pub mod obligation_tests {
    use super::*;
    use crate::launch::fleet::{acceptance_register, status::NoteFacts};

    #[test]
    fn obligation_write_refuses_inconsistent_scope_and_non_acceptance_authors() -> anyhow::Result<()>
    {
        let acceptance = Uuid::from_u128(1);
        let fleet = Uuid::from_u128(2).to_string();
        let def: FleetDefinition = serde_json::from_value(json!({
            "acceptance": {"session": acceptance},
            "criteria": {"C1": {"text": "one"}, "C2": {"text": "two"}},
            "tasks": {"T1": {"criteria": ["C1"]}, "T2": {"criteria": ["C2"]}}
        }))?;
        let mut note = NoteFacts {
            note_uuid: Uuid::from_u128(3),
            session_uuid: acceptance,
            role: None,
            seat: None,
            title: "Regression".into(),
            body: String::new(),
            status: None,
            kind: "task".into(),
            tags: vec!["acceptance-obligation".into(), format!("fleet:{fleet}")],
            data: json!({"obligation_id": "regression", "task": "T2", "criteria": ["C2"], "outcome": "failed"}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert!(acceptance_register::validate_write(&fleet, &def, &note).is_ok());
        note.data["criteria"] = json!(["C1"]);
        assert!(acceptance_register::validate_write(&fleet, &def, &note).is_err());
        note.data["criteria"] = json!(["C2"]);
        note.session_uuid = Uuid::from_u128(4);
        assert!(acceptance_register::validate_write(&fleet, &def, &note).is_err());
        note.session_uuid = acceptance;
        note.data["tasks"] = json!([4]);
        assert!(acceptance_register::validate_write(&fleet, &def, &note).is_err());
        Ok(())
    }
}
