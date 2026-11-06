use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv};
use crate::launch::backend::Notice;

const SOURCE: &str = "mike";

pub(super) async fn report(cli: &HerdrCli, kind: &str, target: &str, values: &[(String, String)]) {
    let mut args = argv(&[kind, "report-metadata", target, "--source", SOURCE]);
    for (name, value) in values {
        args.push("--token".to_string());
        args.push(format!("{name}={value}"));
    }
    if let Err(err) = cli.run_json(&args, DEFAULT_TIMEOUT).await {
        tracing::debug!(%kind, %target, %err, "herdr did not take the fleet's display values");
    }
}

pub(super) async fn notify(cli: &HerdrCli, notice: &Notice) {
    let sound = if notice.urgent { "request" } else { "done" };
    let args = argv(&[
        "notification",
        "show",
        &notice.title,
        "--body",
        &notice.body,
        "--position",
        "top-right",
        "--sound",
        sound,
    ]);
    if let Err(err) = cli.run_json(&args, DEFAULT_TIMEOUT).await {
        tracing::debug!(title = %notice.title, %err, "herdr did not show the fleet's notice");
    }
}
