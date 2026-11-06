use anyhow::{Context, Result};

use super::BackfillLessonInjectionsArgs;
use crate::config::{CliOverrides, load};
use crate::reminders::lesson_injection_backfill::{
    LessonMatchCandidate, extract_lesson_reminder_blocks, matching_lesson_ids,
};
use crate::store::persistence::lesson_injections::{
    BackfillInsertRow, LessonInjectionBackfillCandidate,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn backfill_lesson_injections(
    args: BackfillLessonInjectionsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config =
        load::load(cli).context("failed to resolve config for backfill-lesson-injections")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!(
            "backfill-lesson-injections requires a database_url — persistence is not configured \
             (set it in /settings or via MIKE_DATABASE_URL)"
        );
    };

    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("backfill-lesson-injections: could not connect to the target database")?;

    let candidates = handle
        .list_lesson_injection_backfill_candidates(args.from, args.to)
        .await
        .context("backfill-lesson-injections: failed to list candidate requests")?;
    let lessons = handle
        .list_all_lessons_for_backfill_matching()
        .await
        .context("backfill-lesson-injections: failed to list lessons")?;
    let match_candidates: Vec<LessonMatchCandidate> = lessons
        .into_iter()
        .map(|l| LessonMatchCandidate {
            id: l.id,
            kind: l.kind,
            text: l.text,
        })
        .collect();

    println!(
        "Scanning {} candidate request(s) against {} lesson(s)...",
        candidates.len(),
        match_candidates.len(),
    );

    let scan = scan_lesson_injection_candidates(&handle, &candidates, &match_candidates).await;

    let links_found = scan.insert_rows.len();
    let distinct_lessons = scan
        .insert_rows
        .iter()
        .map(|r| r.lesson_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let distinct_requests = scan
        .insert_rows
        .iter()
        .map(|r| r.request_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let links_inserted = if args.dry_run {
        0
    } else {
        handle
            .insert_backfilled_lesson_injections(&scan.insert_rows)
            .await
            .context("backfill-lesson-injections: failed to insert reconstructed links")?
    };

    print_lesson_injection_backfill_report(
        &scan,
        links_found,
        distinct_lessons,
        distinct_requests,
        links_inserted,
        args.dry_run,
    );

    Ok(())
}

struct LessonInjectionScan {
    requests_scanned: usize,
    requests_with_block: usize,
    requests_no_lesson_matched: usize,
    requests_reconstruction_failed: usize,
    insert_rows: Vec<BackfillInsertRow>,
}

async fn scan_lesson_injection_candidates(
    handle: &PersistHandle,
    candidates: &[LessonInjectionBackfillCandidate],
    match_candidates: &[LessonMatchCandidate],
) -> LessonInjectionScan {
    let mut scan = LessonInjectionScan {
        requests_scanned: 0,
        requests_with_block: 0,
        requests_no_lesson_matched: 0,
        requests_reconstruction_failed: 0,
        insert_rows: Vec::new(),
    };

    for candidate in candidates {
        scan.requests_scanned += 1;
        let request_id = u64::try_from(candidate.request_id).unwrap_or(u64::MAX);
        let sent_body = match handle.fetch_request_sent_body_by_id(request_id).await {
            Ok(body) => body,
            Err(e) => {
                log_sent_body_reconstruction_failed(candidate.request_id, &e);
                scan.requests_reconstruction_failed += 1;
                continue;
            }
        };
        let Some(sent_body) = sent_body else {
            continue;
        };
        let blocks = extract_lesson_reminder_blocks(&sent_body);
        if blocks.is_empty() {
            continue;
        }
        scan.requests_with_block += 1;
        let matched_ids = matching_lesson_ids(&blocks, match_candidates);
        if matched_ids.is_empty() {
            scan.requests_no_lesson_matched += 1;
            continue;
        }
        for lesson_id in matched_ids {
            scan.insert_rows.push(BackfillInsertRow {
                lesson_id,
                request_id: candidate.request_id,
                session_id: candidate.session_id,
                injected_at: candidate.created_at,
                lane: candidate.lane.clone(),
                persona: candidate.persona.clone(),
            });
        }
    }

    scan
}

fn log_sent_body_reconstruction_failed(request_id: i64, e: &PersistError) {
    tracing::warn!(
        request_id,
        error = %e,
        "backfill-lesson-injections: could not reconstruct sent body, skipping"
    );
}

fn print_lesson_injection_backfill_report(
    scan: &LessonInjectionScan,
    links_found: usize,
    distinct_lessons: usize,
    distinct_requests: usize,
    links_inserted: u64,
    dry_run: bool,
) {
    let pct_of_candidates = if scan.requests_scanned > 0 {
        100.0 * scan.requests_no_lesson_matched as f64 / scan.requests_scanned as f64
    } else {
        0.0
    };
    let pct_of_blocks = if scan.requests_with_block > 0 {
        100.0 * scan.requests_no_lesson_matched as f64 / scan.requests_with_block as f64
    } else {
        0.0
    };

    println!(
        "\nbackfill-lesson-injections summary:\n\
         requests scanned: {}\n\
         requests where the sent body could not be reconstructed (skipped): {}\n\
         requests with a proxy-lessons block: {}\n\
         block present but no lesson matched: {} \
         ({pct_of_candidates:.1}% of candidates scanned, {pct_of_blocks:.1}% of blocks found)\n\
         links found: {links_found} ({distinct_lessons} distinct lesson(s), \
         {distinct_requests} distinct request(s))",
        scan.requests_scanned,
        scan.requests_reconstruction_failed,
        scan.requests_with_block,
        scan.requests_no_lesson_matched,
    );
    if dry_run {
        println!("links inserted: 0 (--dry-run; would insert up to {links_found})");
    } else {
        println!("links inserted: {links_inserted}");
    }

    if scan.requests_no_lesson_matched > 0 {
        println!(
            "\nWARNING: {} request(s) had a proxy-lessons block but matched no lesson. This \
             usually signals a matching-logic defect, not that the lessons are gone — \
             investigate before trusting the counts above.",
            scan.requests_no_lesson_matched
        );
    }
}
