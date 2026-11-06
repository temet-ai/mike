use anyhow::{Context, Result};

use super::BackfillToolDefinitionsArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn backfill_tool_definitions(
    args: BackfillToolDefinitionsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for backfill-tool-definitions")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-tool-definitions requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-tool-definitions: could not connect to the target database")?;

    println!(
        "Backfilling tools.definitions from request_id={} through {} (batch size {})...",
        args.from,
        args.to
            .map(|t| t.to_string())
            .unwrap_or_else(|| "end".to_string()),
        args.batch_size,
    );

    let mut cursor = args.from;
    let mut total_requests = 0usize;
    let mut total_rows_updated = 0usize;
    let mut total_missing_body = 0usize;

    loop {
        let batch = handle
            .backfill_tool_definitions_candidates(cursor, args.to, args.batch_size)
            .await
            .context("backfill-tool-definitions: failed to load candidate batch")?;
        let Some(&watermark) = batch.last() else {
            break;
        };

        for request_id in &batch {
            total_requests += 1;
            let outcome = handle
                .backfill_apply_tool_definitions_for_request(*request_id)
                .await
                .with_context(|| {
                    format!("backfill-tool-definitions: failed to apply for request {request_id}")
                })?;
            match outcome {
                Some(rows_updated) => total_rows_updated += rows_updated,
                None => total_missing_body += 1,
            }
        }

        println!(
            "  processed through request_id={watermark} ({total_requests} request(s) scanned, \
             {total_rows_updated} requests.tools row(s) updated, {total_missing_body} skipped \
             — no request_body)"
        );
        cursor = watermark + 1;
    }

    println!(
        "\nbackfill-tool-definitions summary: {total_requests} request(s) scanned, \
         {total_rows_updated} requests.tools row(s) updated, {total_missing_body} skipped \
         (no request_body)"
    );
    Ok(())
}
