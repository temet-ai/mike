use crate::finding::Finding;
use crate::tui::screens::setup::step::Cx;

pub(super) async fn ensure_proxy(cx: &mut Cx<'_>) -> Option<Finding> {
    if cx.client.health().await {
        return None;
    }
    if crate::sandbox::active() {
        return Some(Finding::note(format!(
            "The proxy is not running, and starting it is switched off while {} is set.",
            crate::sandbox::ENV_VAR
        )));
    }
    Some(match crate::stack::ensure_up(cx.config).await {
        Ok(_) => Finding::ok("The proxy was not running, so setup started it."),
        Err(e) => Finding::note(format!("The proxy could not be started: {e:#}")),
    })
}
