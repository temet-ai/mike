use mike::runtime::codex::history::{
    HistoryAction, HistoryBlocker, HistoryIdentity, HistoryPage, HistoryQuery, next_history_step,
};
use mike::runtime::codex::{Field, RequestId};
use serde_json::{Value, json};
use uuid::Uuid;

use super::history_support::{Ledger, frame, identity, items, turns};

#[test]
fn history_empty_and_short_pages_continue_with_complete_opaque_cursor() -> anyhow::Result<()> {
    for data in [json!([]), json!([{"id":"one"}])] {
        for cursor in ["", " A保留\0 "] {
            let request = turns(None)?;
            let observation =
                frame(json!({"data":data,"nextCursor":cursor,"backwardsCursor":false}))?;
            let page = HistoryPage::decode(&observation, &request)?;
            assert_eq!(page.backwards_cursor(), &Field::Invalid(json!(false)));
            let HistoryAction::ReadPage(query) =
                next_history_step(&page, &Ledger::default()).action
            else {
                anyhow::bail!("expected continuation");
            };
            assert_eq!(query.cursor(), Some(cursor));
            assert_eq!(query.scope(), request.query().scope());
            assert_eq!(query.identity(), request.query().identity());
            assert_eq!(query.limit(), request.query().limit());
            assert_eq!(
                query.request(RequestId::Integer(8)).wire_value()["params"]["cursor"],
                cursor
            );
        }
    }
    Ok(())
}

#[test]
fn history_terminal_missing_and_invalid_cursor_are_distinct() -> anyhow::Result<()> {
    let request = items(None)?;
    for (result, action) in [
        (
            json!({"data":[],"nextCursor":null}),
            HistoryAction::ScopeEndObserved,
        ),
        (json!({"data":[]}), HistoryAction::EndUndetermined),
        (
            json!({"data":[],"nextCursor":3}),
            HistoryAction::Blocked(HistoryBlocker::InvalidNextCursor),
        ),
        (
            json!({"data":[],"nextCursor":{"type":"item","itemId":"anchor"}}),
            HistoryAction::Blocked(HistoryBlocker::InvalidNextCursor),
        ),
    ] {
        let observation = frame(result)?;
        let page = HistoryPage::decode(&observation, &request)?;
        assert_eq!(next_history_step(&page, &Ledger::default()).action, action);
    }
    Ok(())
}

#[test]
fn history_cycle_detection_checks_immediate_and_older_committed_cursors() -> anyhow::Result<()> {
    for cursor in ["A", "", " a\0 "] {
        let request = turns(Some(cursor))?;
        let observation = frame(json!({"data":[],"nextCursor":cursor}))?;
        let page = HistoryPage::decode(&observation, &request)?;
        assert_eq!(
            next_history_step(&page, &Ledger::default()).action,
            HistoryAction::Blocked(HistoryBlocker::CursorCycle)
        );
    }
    let request = turns(Some("B"))?;
    let ledger = Ledger {
        committed: vec![(turns(Some("A"))?.query().clone(), "A".into())],
        unavailable: false,
    };
    let observation = frame(json!({"data":[],"nextCursor":"A"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    assert_eq!(
        next_history_step(&page, &ledger).action,
        HistoryAction::Blocked(HistoryBlocker::CursorCycle)
    );
    Ok(())
}

#[test]
fn history_cursor_lookup_unavailable_blocks_without_guessing() -> anyhow::Result<()> {
    let request = turns(None)?;
    let observation = frame(json!({"data":[],"nextCursor":"A"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    let ledger = Ledger {
        unavailable: true,
        ..Ledger::default()
    };
    assert_eq!(
        next_history_step(&page, &ledger).action,
        HistoryAction::Blocked(HistoryBlocker::CursorHistoryUnavailable)
    );
    Ok(())
}

#[test]
fn history_cursor_cycles_are_scoped_by_scan_thread_method_turn_and_limit() -> anyhow::Result<()> {
    let request = items(Some("B"))?;
    let observation = frame(json!({"data":[],"nextCursor":"A"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    let other_scan = HistoryIdentity::new(
        Uuid::from_u128(901),
        Uuid::from_u128(42),
        "Thread:保留 ".into(),
    )?;
    let other_thread =
        HistoryIdentity::new(Uuid::from_u128(900), Uuid::from_u128(42), "other".into())?;
    let other_namespace = HistoryIdentity::new(
        Uuid::from_u128(900),
        Uuid::from_u128(44),
        "Thread:保留 ".into(),
    )?;
    let queries = [
        turns(None)?.query().clone(),
        HistoryQuery::items(identity()?, "Turn:B".into(), 2, None)?,
        HistoryQuery::items(identity()?, "Turn:A".into(), 3, None)?,
        HistoryQuery::items(other_scan, "Turn:A".into(), 2, None)?,
        HistoryQuery::items(other_thread, "Turn:A".into(), 2, None)?,
        HistoryQuery::items(other_namespace, "Turn:A".into(), 2, None)?,
    ];
    for query in queries {
        let ledger = Ledger {
            committed: vec![(query, "A".into())],
            unavailable: false,
        };
        assert!(matches!(
            next_history_step(&page, &ledger).action,
            HistoryAction::ReadPage(_)
        ));
    }
    Ok(())
}

#[test]
fn history_same_uncommitted_page_can_be_evaluated_again_without_accumulation() -> anyhow::Result<()>
{
    let request = turns(None)?;
    let observation = frame(json!({"data":[{"id":"repeat"},{"id":"repeat"}],"nextCursor":"A"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    let ledger = Ledger::default();
    let first = next_history_step(&page, &ledger);
    assert_eq!(next_history_step(&page, &ledger), first);
    assert_eq!(page.entries().count(), 2);
    assert!(ledger.committed.is_empty());
    assert_eq!(page.raw_payload(), observation.raw_payload);
    Ok(())
}

#[test]
fn history_metadata_retains_ephemeral_unknown_null_and_invalid_evidence() -> anyhow::Result<()> {
    let request = HistoryQuery::metadata(identity()?, 2)?.request(RequestId::Integer(7));
    for (mode, ephemeral, uncertain, is_ephemeral) in [
        (json!("legacy"), json!(false), false, false),
        (json!("paginated"), json!(true), false, true),
        (json!("future"), json!(false), true, false),
        (Value::Null, Value::Null, true, false),
        (json!({"unknown":true}), json!("false"), true, false),
    ] {
        let observation = frame(
            json!({"thread":{"id":"Thread:保留 ","historyMode":mode,"ephemeral":ephemeral}}),
        )?;
        let page = HistoryPage::decode(&observation, &request)?;
        let step = next_history_step(&page, &Ledger::default());
        assert_eq!(step.coverage.uncertain_metadata, uncertain);
        assert_eq!(step.coverage.ephemeral, is_ephemeral);
        assert!(matches!(step.action, HistoryAction::ReadTurns(_)));
        assert_eq!(page.provider_result()["thread"]["historyMode"], mode);
    }
    Ok(())
}
