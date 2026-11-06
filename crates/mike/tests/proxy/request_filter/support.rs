use crate::filter_proxy;

use filter_proxy::{StartedFilterProxy, start_proxy_with_rules};
use serde_json::{Value, json};

pub(super) fn drop_system_reminder_rule() -> Value {
    json!({"kind": "xml_tag", "value": "system-reminder", "action": "drop"})
}

pub(super) async fn start_proxy_dropping_system_reminder(
    upstream_url: &str,
) -> anyhow::Result<Option<StartedFilterProxy>> {
    start_proxy_with_rules(upstream_url, &[drop_system_reminder_rule()]).await
}
