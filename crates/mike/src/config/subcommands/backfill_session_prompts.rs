use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;

use super::{BackfillSessionPromptsArgs, RepairSessionPromptsArgs};
use crate::config::{CliOverrides, load};
use crate::store::persistence::session_prompt_lane_backfill::{
    BackfillMainCandidate, BackfillSubagentCandidate,
};
use crate::store::persistence::{self, PersistHandle};

pub(super) async fn backfill_session_prompts(
    args: BackfillSessionPromptsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for backfill-session-prompts")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-session-prompts requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-session-prompts: could not connect to the target database")?;

    println!(
        "Scanning for session_prompts backfill candidates (max body {} bytes)...",
        args.max_body_bytes
    );

    let main_candidates = handle
        .backfill_main_lane_candidates(args.max_body_bytes)
        .await
        .context("backfill-session-prompts: failed to load main-lane candidates")?;
    let subagent_candidates = handle
        .backfill_subagent_lane_candidates(args.max_body_bytes)
        .await
        .context("backfill-session-prompts: failed to load subagent-lane candidates")?;

    println!(
        "Found {} main-lane candidate session(s), {} subagent-lane candidate request(s)",
        main_candidates.len(),
        subagent_candidates.len()
    );

    let request_ids: Vec<i64> = main_candidates
        .iter()
        .map(|c| c.request_id)
        .chain(subagent_candidates.iter().map(|c| c.request_id))
        .collect();
    let bodies = handle
        .backfill_fetch_request_bodies(&request_ids)
        .await
        .context("backfill-session-prompts: failed to fetch candidate request bodies")?;

    let main = insert_main_lane_backfill_rows(&handle, &main_candidates, &bodies).await?;
    let subagent =
        insert_subagent_lane_backfill_rows(&handle, &subagent_candidates, &bodies).await?;

    print_backfill_session_prompts_report(
        main_candidates.len(),
        &main,
        subagent_candidates.len(),
        &subagent,
    );

    Ok(())
}

async fn insert_main_lane_backfill_rows(
    handle: &PersistHandle,
    candidates: &[BackfillMainCandidate],
    bodies: &HashMap<i64, Vec<u8>>,
) -> Result<BackfillTally> {
    let mut tally = BackfillTally::default();
    for candidate in candidates {
        match classify_candidate(bodies.get(&candidate.request_id)) {
            CandidateOutcome::Ready {
                blocks,
                content_sha256,
            } => {
                let inserted = handle
                    .backfill_insert_main_lane_row(
                        candidate.session_id,
                        candidate.request_id,
                        blocks,
                        content_sha256,
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "backfill-session-prompts: failed to insert main-lane row for \
                             session {}",
                            candidate.session_id
                        )
                    })?;
                tally.record(inserted);
            }
            CandidateOutcome::MissingBody => tally.missing_body += 1,
            CandidateOutcome::NoSystemArray => tally.no_system_array += 1,
        }
    }
    Ok(tally)
}

async fn insert_subagent_lane_backfill_rows(
    handle: &PersistHandle,
    candidates: &[BackfillSubagentCandidate],
    bodies: &HashMap<i64, Vec<u8>>,
) -> Result<BackfillTally> {
    let mut tally = BackfillTally::default();
    for candidate in candidates {
        match classify_candidate(bodies.get(&candidate.request_id)) {
            CandidateOutcome::Ready {
                blocks,
                content_sha256,
            } => {
                let inserted = handle
                    .backfill_insert_subagent_lane_row(candidate, blocks, content_sha256)
                    .await
                    .with_context(|| {
                        format!(
                            "backfill-session-prompts: failed to insert subagent-lane row for \
                             request {}",
                            candidate.request_id
                        )
                    })?;
                tally.record(inserted);
            }
            CandidateOutcome::MissingBody => tally.missing_body += 1,
            CandidateOutcome::NoSystemArray => tally.no_system_array += 1,
        }
    }
    Ok(tally)
}

fn print_backfill_session_prompts_report(
    main_count: usize,
    main: &BackfillTally,
    subagent_count: usize,
    subagent: &BackfillTally,
) {
    println!("\nbackfill-session-prompts summary:");
    println!(
        "  main lane:     {} candidate(s), {} row(s) inserted, {} skipped (no system array), \
         {} skipped (body missing)",
        main_count, main.inserted, main.no_system_array, main.missing_body
    );
    println!(
        "  subagent lane: {} candidate(s), {} row(s) inserted, {} skipped (no system array), \
         {} skipped (body missing)",
        subagent_count, subagent.inserted, subagent.no_system_array, subagent.missing_body
    );
}

#[derive(Debug, Default)]
struct BackfillTally {
    inserted: usize,
    no_system_array: usize,
    missing_body: usize,
}

impl BackfillTally {
    fn record(&mut self, inserted: bool) {
        if inserted {
            self.inserted += 1;
        }
    }
}

enum CandidateOutcome {
    Ready {
        blocks: JsonValue,
        content_sha256: Option<String>,
    },
    MissingBody,
    NoSystemArray,
}

fn classify_candidate(body: Option<&Vec<u8>>) -> CandidateOutcome {
    let Some(body) = body else {
        return CandidateOutcome::MissingBody;
    };
    match persistence::extract_system_blocks_from_body(body) {
        Some(blocks) => {
            let content_sha256 = persistence::session_prompt_content_sha256(&blocks);
            CandidateOutcome::Ready {
                blocks,
                content_sha256,
            }
        }
        None => CandidateOutcome::NoSystemArray,
    }
}

pub(super) async fn repair_session_prompts(
    args: RepairSessionPromptsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for repair-session-prompts")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "repair-session-prompts requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("repair-session-prompts: could not connect to the target database")?;

    let candidates = handle
        .find_misattributed_main_lane_rows()
        .await
        .context("repair-session-prompts: failed to scan for misattributed rows")?;

    println!(
        "Found {} main-lane row(s) naming a request other than their session's recurring prompt.",
        candidates.len()
    );
    for c in &candidates {
        println!(
            "  session {} — ledger row {} points at request {}, recurring prompt is on request {} \
             ({} primary-lane turn(s))",
            c.session_id,
            c.ledger_id,
            c.current_request_id,
            c.winner_request_id,
            c.winner_occurrences,
        );
    }

    let outcome = handle
        .repair_main_lane_session_prompts(args.dry_run)
        .await
        .context("repair-session-prompts: repair failed")?;

    let verb = if args.dry_run {
        "would re-point"
    } else {
        "re-pointed"
    };
    println!(
        "\nrepair-session-prompts summary: {verb} {} row(s); {} left alone for want of a second \
         carrier; {} left alone for want of a stored block payload",
        outcome.repaired, outcome.uncorroborated, outcome.no_blocks
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_candidate_missing_body_is_missing_body() {
        assert!(matches!(
            classify_candidate(None),
            CandidateOutcome::MissingBody
        ));
    }

    #[test]
    fn classify_candidate_body_without_system_array_is_no_system_array() -> Result<()> {
        let body = serde_json::to_vec(&serde_json::json!({"messages": []}))?;
        assert!(matches!(
            classify_candidate(Some(&body)),
            CandidateOutcome::NoSystemArray
        ));
        Ok(())
    }

    #[test]
    fn classify_candidate_body_with_system_array_is_ready() -> Result<()> {
        let body = serde_json::to_vec(&serde_json::json!({
            "system": [{"type": "text", "text": "hello"}],
        }))?;
        let CandidateOutcome::Ready {
            blocks,
            content_sha256,
        } = classify_candidate(Some(&body))
        else {
            anyhow::bail!("expected Ready outcome");
        };
        assert!(blocks.is_array());
        assert!(content_sha256.is_some());
        Ok(())
    }
}
