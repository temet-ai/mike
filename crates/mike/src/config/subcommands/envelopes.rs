use anyhow::{Context, Result};

use super::{BackfillRequestEnvelopesArgs, CompactRequestBodiesArgs};
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn backfill_request_envelopes(
    args: BackfillRequestEnvelopesArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for backfill-request-envelopes")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-request-envelopes requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-request-envelopes: could not connect to the target database")?;

    println!(
        "Capturing request_envelope from request_id={} through {} (batch size {})...",
        args.from,
        args.to
            .map(|t| t.to_string())
            .unwrap_or_else(|| "end".to_string()),
        args.batch_size,
    );

    let mut cursor = args.from;
    let mut total_scanned = 0usize;
    let mut total_updated = 0usize;
    let mut total_unparseable = 0usize;
    let mut total_cache_control = 0usize;

    loop {
        let Some(batch) = handle
            .backfill_request_envelope_batch(cursor, args.to, args.batch_size)
            .await
            .context("backfill-request-envelopes: failed to process a batch")?
        else {
            break;
        };
        total_scanned += batch.scanned;
        total_updated += batch.updated;
        total_unparseable += batch.unparseable;
        total_cache_control += batch.cache_control_recovered;
        println!(
            "  processed through request_id={} ({total_scanned} scanned, {total_updated} \
             envelope(s) captured, {total_cache_control} cache_control placement(s) \
             recovered, {total_unparseable} skipped — body is not a JSON object)",
            batch.watermark,
        );
        cursor = batch.watermark + 1;
    }

    println!(
        "Done. {total_scanned} row(s) scanned, {total_updated} envelope(s) captured, \
         {total_cache_control} cache_control placement(s) recovered, {total_unparseable} \
         skipped as unparseable."
    );
    if total_scanned == 0 {
        println!(
            "  No candidates in range — every row here already has an envelope, or has no body."
        );
    }
    Ok(())
}

pub(super) async fn compact_request_bodies(
    args: CompactRequestBodiesArgs,
    cli: CliOverrides,
) -> Result<()> {
    if args.min_idle_days < 0 {
        anyhow::bail!("compact-request-bodies: --min-idle-days must not be negative");
    }
    if args.batch_size <= 0 {
        anyhow::bail!("compact-request-bodies: --batch-size must be greater than zero");
    }

    let config = load::load(cli).context("failed to resolve config for compact-request-bodies")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "compact-request-bodies requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("compact-request-bodies: could not connect to the target database")?;

    let survey = handle
        .survey_request_body_compaction(args.min_idle_days)
        .await
        .context("compact-request-bodies: survey failed")?;

    println!(
        "Compaction survey (sessions idle at least {} day(s), never the session's final request):",
        args.min_idle_days
    );
    println!("  candidate rows        : {}", survey.candidate_rows);
    println!("  sessions affected     : {}", survey.sessions_affected);
    println!(
        "  reclaimable           : {} byte(s)",
        survey.reclaimable_bytes
    );
    println!(
        "  refused, no envelope  : {} row(s), {} byte(s) — these keep their body",
        survey.refused_no_envelope_rows, survey.refused_no_envelope_bytes
    );
    if survey.refused_no_envelope_rows > 0 {
        println!(
            "  (run `mike backfill-request-envelopes` to make the refused rows eligible; \
             without an envelope their system prompt and envelope-only fields exist nowhere else)"
        );
    }

    if !args.execute {
        println!(
            "\nDry run only — pass --execute to clear these bodies. Nothing was written. \
             Run exactly one instance when you do: concurrent runners fight for row locks."
        );
        return Ok(());
    }

    println!("\nClearing (batch size {})...", args.batch_size);
    let outcome = handle
        .compact_request_bodies(args.min_idle_days, args.batch_size)
        .await
        .context("compact-request-bodies: batch clear failed")?;

    println!(
        "\ncompact-request-bodies summary: {} row(s) cleared, {} byte(s) reclaimed across {} \
         batch(es). {} row(s) refused for want of an envelope.",
        outcome.cleared_rows,
        outcome.reclaimed_bytes,
        outcome.batches,
        survey.refused_no_envelope_rows,
    );
    println!(
        "  Space returns to the filesystem only after a table rewrite — \
         `mike reclaim-space --table requests.requests` takes an ACCESS EXCLUSIVE lock, so run it \
         against a quiet proxy."
    );
    Ok(())
}
