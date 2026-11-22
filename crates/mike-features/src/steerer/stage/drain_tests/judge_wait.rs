use std::time::Duration;

use tokio_util::task::TaskTracker;

use crate::steerer::stage::within_wait_or_later;

#[tokio::test]
async fn a_fast_judge_answers_within_the_wait() {
    let tracker = TaskTracker::new();
    let got = within_wait_or_later(
        &tracker,
        Duration::from_millis(500),
        async { Some(7u32) },
        |_| async { panic!("a value ready in time is never sent later") },
    )
    .await;
    assert_eq!(got, Some(7));
}

#[tokio::test]
async fn a_slow_judge_releases_the_request_and_delivers_later() {
    let tracker = TaskTracker::new();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let started = tokio::time::Instant::now();
    let got = within_wait_or_later(
        &tracker,
        Duration::from_millis(50),
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Some(9u32)
        },
        move |value| async move {
            let _ = tx.send(value);
        },
    )
    .await;
    assert_eq!(got, None, "the request does not wait for a slow judge");
    assert!(started.elapsed() < Duration::from_millis(250));
    let later = tokio::time::timeout(Duration::from_secs(2), rx).await;
    assert_eq!(later.ok().and_then(Result::ok), Some(9), "the verdict is kept");
}
