#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use serde_json::json;

use super::{Opened, close_with, entry, opened};
use crate::launch::backend::fake::FakeBackend;

fn run_surface(reused: bool) -> crate::launch::Surface {
    let mut surface = FakeBackend::surface(None);
    surface.workspace_ref = Some("w5".into());
    surface.surface_ref = "w5:p1".into();
    surface.reused = reused;
    surface
}

#[test]
fn the_start_report_records_the_status_pane_and_the_stop_reads_it_back() {
    let report = json!([
        entry(&run_surface(false), Some("recorded late".into())),
        {"seat": "coordinator", "opened": true}
    ]);
    assert_eq!(
        opened(Some(&report)),
        Some(Opened {
            surface: "w5:p1".into(),
            workspace: Some("w5".into()),
            created: true,
        })
    );
    assert_eq!(report[0]["problem"], "recorded late");
    let placed_in_an_existing_workspace = json!([entry(&run_surface(true), None)]);
    assert!(
        !opened(Some(&placed_in_an_existing_workspace))
            .unwrap()
            .created
    );
}

#[test]
fn a_report_without_an_open_status_pane_closes_nothing() {
    assert_eq!(opened(None), None);
    assert_eq!(opened(Some(&json!([]))), None);
    let refused = json!([{"seat": "status", "opened": false, "problem": "no backend"}]);
    assert_eq!(opened(Some(&refused)), None);
    let before_this_change = json!([{"seat": "status", "opened": true, "problem": null}]);
    assert_eq!(opened(Some(&before_this_change)), None);
}

#[tokio::test]
async fn the_stop_closes_the_status_pane_through_the_backend() {
    let backend = FakeBackend::available();
    let pane = Opened {
        surface: "w5:p1".into(),
        workspace: Some("w5".into()),
        created: true,
    };
    close_with(&backend, "smoke", &pane).await;
    let closed = backend.closed.lock().await;
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].surface_ref, "w5:p1");
    assert_eq!(closed[0].workspace_ref.as_deref(), Some("w5"));
    assert_eq!(closed[0].session_uuid, None);
}
