use serde_json::json;

use super::plan::{LESSONS_SETTING, Plan};
use crate::feature_toggles::service::FeatureToggleService;
use crate::finding::Finding;
use crate::settings::service::SettingsService;
use crate::tui::client::ApiClient;

pub(crate) async fn apply_via_api(client: &ApiClient, plan: &Plan) -> Vec<Finding> {
    let mut failures = Vec::new();
    for (key, wanted) in &plan.toggles {
        if let Err(why) = client.patch_feature(key, *wanted).await {
            failures.push(Finding::failed(format!("The proxy refused {key}: {why}")));
        }
    }
    if let Err(why) = client
        .patch_setting(LESSONS_SETTING, json!(plan.session_lessons))
        .await
    {
        failures.push(Finding::failed(format!(
            "The proxy refused {LESSONS_SETTING}: {why}"
        )));
    }
    if !failures.is_empty() {
        return failures;
    }
    let mut findings = vec![Finding::ok(format!(
        "Sent {} switches and the session-lessons setting to the running proxy.",
        plan.toggles.len()
    ))];
    findings.push(read_back(client, plan).await);
    findings
}

async fn read_back(client: &ApiClient, plan: &Plan) -> Finding {
    let Some(states) = client.feature_states().await else {
        return Finding::failed("The proxy did not return its switches when they were read back.");
    };
    let differing: Vec<&str> = plan
        .toggles
        .iter()
        .filter(|(key, wanted)| {
            states
                .iter()
                .find(|(state_key, _)| state_key == key)
                .map(|(_, enabled)| *enabled)
                != Some(*wanted)
        })
        .map(|(key, _)| *key)
        .collect();
    if differing.is_empty() {
        Finding::ok(format!(
            "Read the switches back from the proxy: all {} match the choice.",
            plan.toggles.len()
        ))
    } else {
        Finding::failed(format!(
            "After the change these switches differ from the choice: {}.",
            differing.join(", ")
        ))
    }
}

pub(crate) async fn apply_via_services(
    toggles: &FeatureToggleService,
    settings: &SettingsService,
    plan: &Plan,
) -> Vec<Finding> {
    let mut failures = Vec::new();
    for (key, wanted) in &plan.toggles {
        if let Err(e) = toggles.set(key, *wanted).await {
            failures.push(Finding::failed(format!("{key}: {e}")));
        }
    }
    if let Err(e) = settings
        .set(LESSONS_SETTING, json!(plan.session_lessons))
        .await
    {
        failures.push(Finding::failed(format!("{LESSONS_SETTING}: {e}")));
    }
    if failures.is_empty() {
        vec![Finding::ok(format!(
            "Applied {} switches and the session-lessons setting.",
            plan.toggles.len()
        ))]
    } else {
        failures
    }
}
