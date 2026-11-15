use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;

use super::FeatureToggles;

pub const TOGGLE_POLL: Duration = Duration::from_secs(2);

pub async fn wait_until_on(
    toggles: &ArcSwap<FeatureToggles>,
    on: impl Fn(&FeatureToggles) -> bool,
    shutdown: &CancellationToken,
) -> bool {
    loop {
        if on(&toggles.load()) {
            return true;
        }
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return false,
            () = tokio::time::sleep(TOGGLE_POLL) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn wait_until_on_toggle_flipped_on_returns_true() {
        let toggles = Arc::new(ArcSwap::from_pointee(FeatureToggles {
            memory_keeper: false,
            ..FeatureToggles::default()
        }));
        let shutdown = CancellationToken::new();
        let waiter = {
            let toggles = toggles.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(
                async move { wait_until_on(&toggles, |t| t.memory_keeper, &shutdown).await },
            )
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!waiter.is_finished());
        toggles.store(Arc::new(FeatureToggles {
            memory_keeper: true,
            ..FeatureToggles::default()
        }));
        let woke = tokio::time::timeout(TOGGLE_POLL * 2, waiter).await;
        assert!(matches!(woke, Ok(Ok(true))));
    }

    #[tokio::test]
    async fn wait_until_on_shutdown_while_off_returns_false() {
        let toggles = ArcSwap::from_pointee(FeatureToggles {
            memory_keeper: false,
            ..FeatureToggles::default()
        });
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        assert!(!wait_until_on(&toggles, |t| t.memory_keeper, &shutdown).await);
    }
}
