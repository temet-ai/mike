use anyhow::{Context, Result};

use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

pub(super) async fn backfill_flow_admission(cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for backfill-flow-admission")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-flow-admission requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-flow-admission: could not connect to the target database")?;

    println!(
        "Scanning sessions with no Session Flow admission evidence \
         (flow_capture_version IS NULL)..."
    );
    let outcome = handle
        .backfill_admit_complete_capture_sessions()
        .await
        .context("backfill-flow-admission: failed to admit eligible sessions")?;
    let left_alone = outcome.examined - outcome.admitted;

    println!(
        "\nbackfill-flow-admission summary: {} session(s) examined, {} admitted \
         (earliest captured request carried exactly one message), \
         {left_alone} left alone (earliest captured request already had more \
         than one message, or the session has no captured requests at all — \
         these correctly keep returning NoAdmittedStart)",
        outcome.examined, outcome.admitted,
    );
    Ok(())
}
