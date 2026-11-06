use anyhow::{Context, Result};

use super::{RepairSessionPromptsArgs, RestampSidecarOriginsArgs};
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn restamp_sidecar_origins(
    args: RestampSidecarOriginsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for restamp-sidecar-origins")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "restamp-sidecar-origins requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("restamp-sidecar-origins: could not connect to the target database")?;

    println!(
        "Re-running turn-origin fingerprints over primary-lane history (batch size {})...",
        args.batch_size
    );

    let outcome = handle
        .restamp_misclassified_sidecar_origins(args.batch_size, args.dry_run)
        .await
        .context("restamp-sidecar-origins: run failed")?;

    let verb = if args.dry_run {
        "would re-stamp"
    } else {
        "re-stamped"
    };
    println!(
        "\nrestamp-sidecar-origins summary: {verb} {} row(s), {} left as primary, {} batch(es)",
        outcome.restamped, outcome.unchanged, outcome.batches
    );

    Ok(())
}

pub(super) async fn demote_duplicate_first_turns(
    args: RepairSessionPromptsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for demote-duplicate-first-turns")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "demote-duplicate-first-turns requires a database_url — persistence is not \
             configured (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("demote-duplicate-first-turns: could not connect to the target database")?;

    let affected = handle
        .demote_duplicate_first_turns(args.dry_run)
        .await
        .context("demote-duplicate-first-turns: run failed")?;

    let verb = if args.dry_run {
        "would demote"
    } else {
        "demoted"
    };
    println!("demote-duplicate-first-turns summary: {verb} {affected} row(s)");
    Ok(())
}
