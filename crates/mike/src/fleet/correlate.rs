use std::collections::HashMap;

use serde_json::Value;

use super::finite_client::RunManifest;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TurnReport {
    pub(crate) ts: String,
    pub(crate) progress: Value,
    pub(crate) blockers: Value,
    pub(crate) findings: Value,
    pub(crate) artifacts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActorNode {
    pub(crate) actor_id: String,
    pub(crate) parent_node_id: Option<String>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) terminal: bool,
    pub(crate) latest_turn_report: Option<TurnReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunEvent {
    pub(crate) event_kind: String,
    pub(crate) ts: String,
    pub(crate) actor_id: String,
    pub(crate) verdict: Option<String>,
    pub(crate) payload: Value,
}

pub(crate) fn correlate_actors(manifest: &RunManifest) -> Vec<ActorNode> {
    let mut order: Vec<String> = Vec::new();
    let mut nodes: HashMap<String, ActorNode> = HashMap::new();

    for entry in &manifest.entries {
        let Some(actor_id) = entry.actor_id.clone() else {
            continue;
        };
        let node = nodes.entry(actor_id.clone()).or_insert_with(|| {
            order.push(actor_id.clone());
            ActorNode {
                actor_id,
                parent_node_id: None,
                session_uuid: None,
                workspace: None,
                model: None,
                terminal: false,
                latest_turn_report: None,
            }
        });

        if node.parent_node_id.is_none() {
            node.parent_node_id = str_field(&entry.payload, "parent_node_id");
        }
        if node.session_uuid.is_none() {
            node.session_uuid = str_field(&entry.payload, "session_uuid");
        }
        if node.workspace.is_none() {
            node.workspace = str_field(&entry.payload, "workspace");
        }
        if node.model.is_none() {
            node.model = str_field(&entry.payload, "model");
        }
        if bool_field(&entry.payload, "terminal") {
            node.terminal = true;
        }
        if entry.event_kind == "turn-report"
            && let Some(report) = extract_turn_report(&entry.payload, entry.ts.as_deref())
        {
            let is_newer = match &node.latest_turn_report {
                None => true,
                Some(prev) => report.ts >= prev.ts,
            };
            if is_newer {
                node.latest_turn_report = Some(report);
            }
        }
    }

    order
        .into_iter()
        .filter_map(|id| nodes.remove(&id))
        .collect()
}

fn extract_turn_report(payload: &Value, ts: Option<&str>) -> Option<TurnReport> {
    let source = payload.get("report").unwrap_or(payload);
    let has_content = ["progress", "blockers", "findings", "artifacts"]
        .iter()
        .any(|key| source.get(key).is_some());
    if !has_content {
        return None;
    }
    Some(TurnReport {
        ts: ts.unwrap_or_default().to_string(),
        progress: source.get("progress").cloned().unwrap_or(Value::Null),
        blockers: source.get("blockers").cloned().unwrap_or(Value::Null),
        findings: source.get("findings").cloned().unwrap_or(Value::Null),
        artifacts: source.get("artifacts").cloned().unwrap_or(Value::Null),
    })
}

const RUN_EVENT_KINDS: [&str; 3] = ["guard-invoked", "guard-verdict", "budget-exhausted"];

pub(crate) fn extract_run_events(manifest: &RunManifest) -> Vec<RunEvent> {
    manifest
        .entries
        .iter()
        .filter(|entry| RUN_EVENT_KINDS.contains(&entry.event_kind.as_str()))
        .map(|entry| RunEvent {
            event_kind: entry.event_kind.clone(),
            ts: entry.ts.clone().unwrap_or_default(),
            actor_id: entry.actor_id.clone().unwrap_or_default(),
            verdict: entry.verdict.clone(),
            payload: entry.payload.clone(),
        })
        .collect()
}

fn bool_field(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn str_field(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fleet::finite_client::LedgerEntry;

    fn entry(event_kind: &str, actor_id: &str, ts: &str, payload: Value) -> LedgerEntry {
        LedgerEntry {
            event_kind: event_kind.to_string(),
            actor_id: Some(actor_id.to_string()),
            ts: Some(ts.to_string()),
            verdict: None,
            payload,
        }
    }

    fn manifest_from(entries: Vec<LedgerEntry>) -> RunManifest {
        RunManifest {
            run_id: "run-1".to_string(),
            entries,
        }
    }

    #[test]
    fn correlate_actors_keys_nodes_on_actor_id_not_parent_node_id() -> anyhow::Result<()> {
        let manifest = manifest_from(vec![
            entry(
                "turn-report",
                "actor-1",
                "2026-07-12T15:44:37.852Z",
                json!({
                    "spawn_ack": true,
                    "parent_node_id": "actor-1",
                    "session_uuid": "2875c812-fc8a-47d6-b3da-6044f5e836d7",
                    "workspace": "workspace:123",
                    "model": "sonnet",
                }),
            ),
            entry(
                "turn-report",
                "2875c812-fc8a-47d6-b3da-6044f5e836d7",
                "2026-07-12T15:45:43.558Z",
                json!({
                    "parent_node_id": "actor-1",
                    "terminal": true,
                    "progress": {"criteria_met": ["scaffold-ci-green"], "criteria_open": [], "criteria_suspect": []},
                    "findings": ["scaffold built"],
                    "blockers": [],
                    "artifacts": [],
                }),
            ),
        ]);

        let actors = correlate_actors(&manifest);
        assert_eq!(
            actors.len(),
            2,
            "orchestrator and worker are distinct nodes"
        );

        let orchestrator = &actors[0];
        assert_eq!(orchestrator.actor_id, "actor-1");
        assert_eq!(orchestrator.parent_node_id.as_deref(), Some("actor-1"));
        assert_eq!(
            orchestrator.session_uuid.as_deref(),
            Some("2875c812-fc8a-47d6-b3da-6044f5e836d7")
        );
        assert!(!orchestrator.terminal);

        let worker = &actors[1];
        assert_eq!(worker.actor_id, "2875c812-fc8a-47d6-b3da-6044f5e836d7");
        assert_eq!(worker.parent_node_id.as_deref(), Some("actor-1"));
        assert!(worker.terminal);
        let report = worker
            .latest_turn_report
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("worker self-reported progress"))?;
        assert_eq!(report.findings, json!(["scaffold built"]));
        Ok(())
    }

    #[test]
    fn correlate_actors_leaves_parent_node_id_none_when_absent_from_payload() -> anyhow::Result<()>
    {
        let manifest = manifest_from(vec![
            entry("turn-report", "actor-1", "t0", json!({"findings": ["x"]})),
            entry(
                "turn-report",
                "actor-1:S2:w0:i0",
                "t1",
                json!({"report": {"findings": ["y"], "progress": {}, "blockers": [], "artifacts": []}}),
            ),
            entry(
                "budget-exhausted",
                "external-watcher",
                "t2",
                json!({"limit": "wallClockMs"}),
            ),
        ]);

        let actors = correlate_actors(&manifest);
        assert_eq!(actors.len(), 3);
        assert!(actors.iter().all(|a| a.parent_node_id.is_none()));

        let sub_worker = actors
            .iter()
            .find(|a| a.actor_id == "actor-1:S2:w0:i0")
            .ok_or_else(|| anyhow::anyhow!("sub-workflow actor present"))?;
        assert_eq!(
            sub_worker
                .latest_turn_report
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("nested report extracted"))?
                .findings,
            json!(["y"]),
            "payload.report nesting is unwrapped"
        );

        let watcher = actors
            .iter()
            .find(|a| a.actor_id == "external-watcher")
            .ok_or_else(|| anyhow::anyhow!("synthetic watcher actor present"))?;
        assert!(watcher.session_uuid.is_none());
        assert!(watcher.latest_turn_report.is_none());
        Ok(())
    }

    #[test]
    fn correlate_actors_keeps_the_latest_turn_report_by_ts() -> anyhow::Result<()> {
        let manifest = manifest_from(vec![
            entry(
                "turn-report",
                "actor-1",
                "2026-07-12T10:00:00.000Z",
                json!({"findings": ["first"], "progress": {}, "blockers": [], "artifacts": []}),
            ),
            entry(
                "turn-report",
                "actor-1",
                "2026-07-12T11:00:00.000Z",
                json!({"findings": ["second"], "progress": {}, "blockers": [], "artifacts": []}),
            ),
        ]);

        let actors = correlate_actors(&manifest);
        assert_eq!(actors.len(), 1);
        let report = actors[0]
            .latest_turn_report
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("latest turn report present"))?;
        assert_eq!(report.findings, json!(["second"]));
        Ok(())
    }

    #[test]
    fn correlate_actors_empty_manifest_returns_empty() {
        assert!(correlate_actors(&manifest_from(vec![])).is_empty());
    }

    #[test]
    fn extract_run_events_filters_to_guard_and_budget_kinds_in_order() {
        let manifest = manifest_from(vec![
            entry("turn-report", "actor-1", "t0", json!({"findings": []})),
            entry(
                "guard-invoked",
                "actor-1",
                "t1",
                json!({"guard": {"kind": "llm"}}),
            ),
            {
                let mut e = entry(
                    "guard-verdict",
                    "actor-1",
                    "t2",
                    json!({"result": {"pass": true}}),
                );
                e.verdict = Some("Success".to_string());
                e
            },
            entry(
                "budget-exhausted",
                "external-watcher",
                "t3",
                json!({"limit": "wallClockMs", "max": 10800000}),
            ),
        ]);

        let events = extract_run_events(&manifest);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_kind, "guard-invoked");
        assert_eq!(events[1].event_kind, "guard-verdict");
        assert_eq!(events[1].verdict.as_deref(), Some("Success"));
        assert_eq!(events[2].event_kind, "budget-exhausted");
        assert_eq!(events[2].actor_id, "external-watcher");
    }
}
