#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ApplyError {
    pub configuration_may_have_changed: bool,
    pub message: &'static str,
}

pub(crate) async fn save_and_reconcile<C>(
    config: &mut C,
    prepare: impl FnOnce(&C) -> Result<C, ApplyError>,
    reconcile: impl AsyncFn(&C) -> anyhow::Result<()>,
) -> Result<(), ApplyError> {
    let reloaded = prepare(config)?;
    *config = reloaded;
    reconcile(config).await.map_err(|_| ApplyError {
        configuration_may_have_changed: true,
        message: "provider key was saved but the selected owned server did not verify; setup remains incomplete; retry or restore the private config backup",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn saved_key_is_reconciled_before_success_is_reported() -> anyhow::Result<()> {
        let mut config = "previous";
        let called = Cell::new(false);
        save_and_reconcile(
            &mut config,
            |_| Ok("saved"),
            async |resolved: &&str| {
                assert_eq!(*resolved, "saved");
                called.set(true);
                Ok(())
            },
        )
        .await?;
        assert!(called.get());
        assert_eq!(config, "saved");
        Ok(())
    }

    #[tokio::test]
    async fn override_or_save_refusal_never_restarts_the_running_server() {
        let mut config = "previous";
        let called = Cell::new(false);
        let result = save_and_reconcile(
            &mut config,
            |_| {
                Err(ApplyError {
                    configuration_may_have_changed: false,
                    message: "conflicting override; no configuration was changed",
                })
            },
            async |_: &&str| {
                called.set(true);
                Ok(())
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(config, "previous");
    }

    #[tokio::test]
    async fn saved_but_unverified_key_stays_pending_and_errors_do_not_echo_secrets()
    -> anyhow::Result<()> {
        let mut config = "previous";
        let error = save_and_reconcile(
            &mut config,
            |_| Ok("saved"),
            async |_: &&str| {
                anyhow::bail!("private-sentinel");
            },
        )
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("unverified runtime was accepted"))?;
        assert!(error.configuration_may_have_changed);
        assert!(!error.to_string().contains("private-sentinel"));
        assert_eq!(config, "saved");
        Ok(())
    }
}
