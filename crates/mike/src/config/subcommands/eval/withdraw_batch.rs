use anyhow::Result;

use super::WithdrawBatchArgs;
use crate::config::CliOverrides;

pub(super) async fn withdraw_batch(args: WithdrawBatchArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::connect(cli, args.execute).await?;
    let by = std::env::var("USER").unwrap_or_else(|_| "mike".to_owned());
    let report = persist
        .withdraw_label_batch(&args.batch, &args.reason, &by, args.execute)
        .await?;
    match (report.executed, report.labels) {
        (false, n) => println!(
            "{n} current label(s) in batch {}; dry run, nothing written",
            args.batch
        ),
        (true, 0) => println!("batch {} is already withdrawn; nothing written", args.batch),
        (true, n) => println!("withdrew {n} label(s) of batch {}", args.batch),
    }
    Ok(())
}
