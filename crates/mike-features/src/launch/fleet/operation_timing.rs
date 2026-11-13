use std::path::Path;

use serde_json::json;

pub fn completed(
    context: &super::managed_command::Context,
    queue_ms: u128,
    execution_ms: u128,
) {
    if let Some(receipt) = context.receipt_id
        && let Err(error) = record(
            &context.private_host_root,
            receipt,
            &context.ledger,
            queue_ms,
            execution_ms,
        )
    {
        tracing::warn!(%error, receipt, "operation timing was not persisted");
    }
}

pub fn record(
    root: &Path,
    receipt: i64,
    ledger: &str,
    queue_ms: u128,
    execution_ms: u128,
) -> std::io::Result<()> {
    let directory = root.join("timings");
    std::fs::create_dir_all(&directory)?;
    let temporary = directory.join(format!("{receipt}-{}.tmp", uuid::Uuid::new_v4()));
    let data = json!({"schema": "operation-timing-v1", "receipt": receipt, "ledger": ledger,
                     "queue_and_setup_ms": queue_ms, "execution_and_cleanup_ms": execution_ms});
    std::fs::write(&temporary, serde_json::to_vec(&data)?)?;
    std::fs::rename(temporary, directory.join(format!("{receipt}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_timing_is_bound_to_receipt_without_command_output() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        record(root.path(), 81, "fleet-one", 12, 30)?;
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.path().join("timings/81.json"))?)?;
        assert_eq!(value["queue_and_setup_ms"], 12);
        assert_eq!(value["receipt"], 81);
        assert_eq!(value["ledger"], "fleet-one");
        assert!(value.get("output").is_none());
        let file = root.path().join("not-a-directory");
        std::fs::write(&file, "occupied")?;
        assert!(record(&file, 1, "fleet-one", 1, 1).is_err());
        Ok(())
    }
}
