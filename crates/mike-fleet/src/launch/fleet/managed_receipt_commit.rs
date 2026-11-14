use std::time::{Duration, Instant};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

const POLL: Duration = Duration::from_millis(100);
const LIMIT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Committed,
    NotCommitted,
}

pub async fn reconcile(
    persist: &PersistHandle,
    receipt: i64,
    transaction: i64,
) -> Result<Outcome, String> {
    let started = Instant::now();
    loop {
        let last = match persist.fleet().transaction_status(transaction).await {
            Ok(Some(status)) if status == "committed" => return Ok(Outcome::Committed),
            Ok(Some(status)) if status == "aborted" => return Ok(Outcome::NotCommitted),
            Ok(Some(_)) => "the transaction is still in progress".to_string(),
            Ok(None) => return by_row(persist, receipt).await,
            Err(error) => error.to_string(),
        };
        if started.elapsed() >= LIMIT {
            return Err(format!(
                "the receipt's commit outcome could not be read back within {} s: {last}",
                LIMIT.as_secs()
            ));
        }
        tokio::time::sleep(POLL).await;
    }
}

async fn by_row(persist: &PersistHandle, receipt: i64) -> Result<Outcome, String> {
    match persist.fleet().tree_check(receipt).await {
        Ok(Some(_)) => Ok(Outcome::Committed),
        Ok(None) => Ok(Outcome::NotCommitted),
        Err(error) => Err(error.to_string()),
    }
}
