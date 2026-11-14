use std::cell::Cell;

use chrono::Utc;

use super::*;

fn row(turn: i64) -> EvalRunItemRow {
    EvalRunItemRow {
        run_id: 1,
        operator_turn_id: turn,
        reaction_label_id: None,
        delivery_label_id: None,
        turn_verdict_id: None,
        outcome: "judged".to_owned(),
        stratum: "all".to_owned(),
        inclusion_prob: None,
        reaction_correct: None,
        delivery_correct: None,
        created_at: Utc::now(),
        gold_reaction: None,
        gold_delivery: None,
        gold_control: None,
        verdict_reaction: None,
        verdict_delivery: None,
        verdict_legacy_label: None,
        next_state: None,
        verdict: None,
        gold_severity: None,
        current_reaction: None,
        current_delivery: None,
        reviewed_at: None,
        session_uuid: None,
        session_title: None,
    }
}

fn prelabel(uncertain: bool) -> Prelabel {
    Prelabel {
        label: "corrected".to_owned(),
        probability: if uncertain { 0.5 } else { 0.95 },
        uncertain,
        record_id: None,
    }
}

fn store(turns: i64) -> Vec<EvalRunItemRow> {
    (1..=turns).map(row).collect()
}

async fn serve(
    all: &[EvalRunItemRow],
    scan: &RunItemsFilter,
) -> Result<RunItemsPage, PersistError> {
    let from = usize::try_from(scan.offset).unwrap_or(0).min(all.len());
    let to = (from + usize::try_from(scan.limit).unwrap_or(0)).min(all.len());
    Ok(RunItemsPage {
        items: all[from..to].to_vec(),
        total: i64::try_from(all.len()).unwrap_or(0),
        reviewed: 7,
    })
}

fn ids(page: &RunItemsPage) -> Vec<i64> {
    page.items.iter().map(|i| i.operator_turn_id).collect()
}

#[test]
fn parse_accepts_uncertain_or_nothing_and_names_a_bad_value() {
    assert_eq!(parse(None), Ok(false));
    assert_eq!(parse(Some("")), Ok(false));
    assert_eq!(parse(Some("uncertain")), Ok(true));
    let Err(message) = parse(Some("all")) else {
        panic!("expected a refusal");
    };
    assert!(message.contains("\"all\""));
}

#[test]
fn needs_review_keeps_uncertain_and_missing_and_drops_confident() {
    let prelabels = BTreeMap::from([(1, prelabel(true)), (2, prelabel(false))]);
    assert!(needs_review(1, &prelabels));
    assert!(!needs_review(2, &prelabels));
    assert!(needs_review(3, &prelabels));
}

#[tokio::test]
async fn uncertain_page_with_no_prelabels_returns_every_item() -> anyhow::Result<()> {
    let all = store(5);
    let filter = RunItemsFilter {
        limit: 50,
        ..RunItemsFilter::default()
    };
    let page = uncertain_page(
        &filter,
        async |scan| serve(&all, scan).await,
        |_| BTreeMap::new(),
    )
    .await?;
    assert_eq!(ids(&page), vec![1, 2, 3, 4, 5]);
    assert_eq!(page.total, 5);
    assert_eq!(page.reviewed, 7);
    Ok(())
}

#[tokio::test]
async fn uncertain_page_drops_confident_items_and_pages_over_the_rest() -> anyhow::Result<()> {
    let all = store(6);
    let lookup = |_: &[i64]| {
        BTreeMap::from([
            (1, prelabel(false)),
            (2, prelabel(true)),
            (4, prelabel(false)),
        ])
    };
    let first = RunItemsFilter {
        limit: 2,
        offset: 0,
        ..RunItemsFilter::default()
    };
    let page = uncertain_page(&first, async |scan| serve(&all, scan).await, lookup).await?;
    assert_eq!(ids(&page), vec![2, 3]);
    assert_eq!(page.total, 4);
    let second = RunItemsFilter {
        limit: 2,
        offset: 2,
        ..RunItemsFilter::default()
    };
    let page = uncertain_page(&second, async |scan| serve(&all, scan).await, lookup).await?;
    assert_eq!(ids(&page), vec![5, 6]);
    Ok(())
}

#[tokio::test]
async fn uncertain_page_reads_every_batch_of_a_long_run() -> anyhow::Result<()> {
    let all = store(BATCH * 2 + 30);
    let calls = Cell::new(0);
    let filter = RunItemsFilter {
        limit: 10,
        offset: 0,
        ..RunItemsFilter::default()
    };
    let page = uncertain_page(
        &filter,
        async |scan| {
            calls.set(calls.get() + 1);
            serve(&all, scan).await
        },
        |_| BTreeMap::new(),
    )
    .await?;
    assert_eq!(calls.get(), 3);
    assert_eq!(page.total, BATCH * 2 + 30);
    assert_eq!(page.items.len(), 10);
    Ok(())
}

#[tokio::test]
async fn uncertain_page_passes_a_failed_read_through() {
    let filter = RunItemsFilter {
        limit: 10,
        ..RunItemsFilter::default()
    };
    let outcome = uncertain_page(
        &filter,
        async |_| Err(PersistError::MissingDatabaseUrl),
        |_| BTreeMap::new(),
    )
    .await;
    assert!(outcome.is_err());
}
