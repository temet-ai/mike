use std::sync::atomic::{AtomicUsize, Ordering};

use super::BundleMemo;

async fn load_counted(
    memo: &BundleMemo<String>,
    loads: &AtomicUsize,
    bundle_id: i64,
    found: bool,
) -> Option<String> {
    memo.get_or_load(bundle_id, || async move {
        loads.fetch_add(1, Ordering::SeqCst);
        found.then(|| format!("bundle-{bundle_id}"))
    })
    .await
}

#[tokio::test]
async fn a_bundle_is_read_once_per_id_across_ticks() -> anyhow::Result<()> {
    let memo = BundleMemo::default();
    let loads = AtomicUsize::new(0);
    for _ in 0..90 {
        let got = load_counted(&memo, &loads, 7, true).await;
        assert_eq!(got.as_deref(), Some("bundle-7"));
    }
    assert_eq!(
        loads.load(Ordering::SeqCst),
        1,
        "an unchanged live bundle id must not be read from eval.bundles again"
    );
    let other = load_counted(&memo, &loads, 8, true).await;
    assert_eq!(other.as_deref(), Some("bundle-8"));
    assert_eq!(loads.load(Ordering::SeqCst), 2, "a new id is read once");
    Ok(())
}

#[tokio::test]
async fn a_failed_load_is_retried() -> anyhow::Result<()> {
    let memo = BundleMemo::default();
    let loads = AtomicUsize::new(0);
    assert!(load_counted(&memo, &loads, 3, false).await.is_none());
    assert_eq!(
        load_counted(&memo, &loads, 3, true).await.as_deref(),
        Some("bundle-3")
    );
    assert_eq!(loads.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn clones_share_one_memo() -> anyhow::Result<()> {
    let memo = BundleMemo::default();
    let loads = AtomicUsize::new(0);
    load_counted(&memo, &loads, 1, true).await;
    load_counted(&memo.clone(), &loads, 1, true).await;
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    Ok(())
}
