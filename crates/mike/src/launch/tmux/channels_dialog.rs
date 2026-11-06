use std::time::{Duration, Instant};

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};

const DIALOG_TEXT: &str = "Loading development channels";
const WAIT_BUDGET: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(500);

pub(super) fn dialog_visible(snapshot: &str) -> bool {
    snapshot.contains(DIALOG_TEXT)
}

async fn snapshot(cli: &TmuxCli, pane: &str) -> String {
    cli.run_text(&argv(&["capture-pane", "-p", "-t", pane]), DEFAULT_TIMEOUT)
        .await
        .unwrap_or_default()
}

pub(super) async fn confirm(cli: &TmuxCli, pane: &str) -> bool {
    let deadline = Instant::now() + WAIT_BUDGET;
    while !dialog_visible(&snapshot(cli, pane).await) {
        if Instant::now() >= deadline {
            eprintln!(
                "mike spawn: pane {pane} never showed the development-channels dialog within \
                 {WAIT_BUDGET:?}; leaving it alone"
            );
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
    let sent = cli
        .run_ok(&argv(&["send-keys", "-t", pane, "Enter"]), DEFAULT_TIMEOUT)
        .await;
    if let Err(error) = &sent {
        eprintln!(
            "mike spawn: saw the development-channels dialog in {pane} but Enter failed: {error}"
        );
    }
    sent.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_text_is_recognised() {
        assert!(dialog_visible(
            "  WARNING: Loading development channels\n  1. I am using this"
        ));
    }

    #[test]
    fn repl_screen_is_not_the_dialog() {
        assert!(!dialog_visible("> \n  bypass permissions on"));
    }
}
