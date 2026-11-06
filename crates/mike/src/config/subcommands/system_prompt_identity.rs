use anyhow::{Context, Result};

use super::BackfillRequestSystemPromptsArgs;
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn backfill_request_system_prompts(
    args: BackfillRequestSystemPromptsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for backfill-request-system-prompts")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-request-system-prompts requires a database_url — persistence is not \
             configured (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-request-system-prompts: could not connect to the target database")?;

    println!(
        "Backfilling requests.system_prompts (batch size {})...",
        args.batch_size
    );

    let outcome = handle
        .backfill_request_system_prompts(args.batch_size)
        .await
        .context("backfill-request-system-prompts: backfill run failed")?;

    println!(
        "\nbackfill-request-system-prompts summary: {} row(s) resolved, {} row(s) unknown_shape, \
         {} batch(es)",
        outcome.resolved, outcome.unknown_shape, outcome.batches
    );

    let uncompactable = handle
        .count_system_prompts_missing_and_uncompactable()
        .await
        .context(
            "backfill-request-system-prompts: failed to count missing-and-uncompactable rows",
        )?;
    println!(
        "accepted-loss count (request_body already NULL, cannot be resolved by this backfill): \
         {uncompactable}"
    );

    Ok(())
}

pub(super) async fn reclassify_system_prompts(
    args: BackfillRequestSystemPromptsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for reclassify-system-prompts")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "reclassify-system-prompts requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("reclassify-system-prompts: could not connect to the target database")?;

    println!(
        "Re-examining unknown-shape rows (batch size {})...",
        args.batch_size
    );

    let outcome = handle
        .reclassify_unknown_shape_system_prompts(args.batch_size)
        .await
        .context("reclassify-system-prompts: run failed")?;

    println!(
        "\nreclassify-system-prompts summary: {} row(s) resolved, {} row(s) still unresolved, \
         {} batch(es)",
        outcome.resolved, outcome.unknown_shape, outcome.batches
    );

    Ok(())
}
