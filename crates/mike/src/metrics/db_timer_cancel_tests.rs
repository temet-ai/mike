use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::oneshot;

use super::{OUTCOME_DROPPED, OUTCOME_ERR, OUTCOME_OK, time_db};
use crate::metrics::names::HISTOGRAM_DB_QUERY_DURATION_MS;

fn samples(handle: &PrometheusHandle, method: &str, outcome: &str) -> Option<u64> {
    let prefix = format!(
        "{HISTOGRAM_DB_QUERY_DURATION_MS}_count{{method=\"{method}\",outcome=\"{outcome}\"}} "
    );
    handle.render().lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .and_then(|n| n.trim().parse().ok())
    })
}

#[tokio::test]
async fn a_completed_wait_records_its_outcome_once_and_returns_its_value() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let ok: Result<u32, &str> = time_db("done_ok", async { Ok(7) }).await;
    let err: Result<u32, &str> = time_db("done_err", async { Err("boom") }).await;
    assert_eq!((ok, err), (Ok(7), Err("boom")));
    assert_eq!(samples(&handle, "done_ok", OUTCOME_OK), Some(1));
    assert_eq!(samples(&handle, "done_err", OUTCOME_ERR), Some(1));
    assert_eq!(samples(&handle, "done_ok", OUTCOME_DROPPED), None);
}

#[tokio::test]
async fn a_wait_dropped_while_pending_records_one_incomplete_sample_and_no_success() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let (started_tx, started_rx) = oneshot::channel::<()>();
    let (_never_tx, never_rx) = oneshot::channel::<()>();
    let wait = time_db("cancelled", async move {
        let _ = started_tx.send(());
        let _ = never_rx.await;
        Ok::<(), &str>(())
    });
    let mut wait = Box::pin(wait);
    tokio::select! {
        biased;
        _ = &mut wait => unreachable!("the inner future never completes"),
        _ = started_rx => {}
    }
    drop(wait);
    assert_eq!(samples(&handle, "cancelled", OUTCOME_DROPPED), Some(1));
    assert_eq!(samples(&handle, "cancelled", OUTCOME_OK), None);
    assert_eq!(samples(&handle, "cancelled", OUTCOME_ERR), None);
}

#[tokio::test]
async fn a_wait_never_polled_records_nothing() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let wait = time_db("never_polled", async { Ok::<(), &str>(()) });
    drop(wait);
    for outcome in [OUTCOME_OK, OUTCOME_ERR, OUTCOME_DROPPED] {
        assert_eq!(samples(&handle, "never_polled", outcome), None);
    }
}

#[tokio::test]
async fn a_wait_that_panics_is_never_recorded_as_a_success() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let wait = time_db("unwound", async {
        let fail = true;
        if fail {
            panic!("query task unwound");
        }
        Ok::<(), &str>(())
    });
    let unwound = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(wait)).await;
    assert!(unwound.is_err());
    assert_eq!(samples(&handle, "unwound", OUTCOME_DROPPED), Some(1));
    assert_eq!(samples(&handle, "unwound", OUTCOME_OK), None);
}
