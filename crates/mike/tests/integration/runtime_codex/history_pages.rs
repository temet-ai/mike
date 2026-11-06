use mike::runtime::codex::history::{
    HistoryAction, HistoryEntry, HistoryErrorKind, HistoryPage, HistoryQuery, next_history_step,
};
use mike::runtime::codex::{EvidenceOrigin, Field, ItemCategory, RequestId};
use serde_json::json;

use super::history_support::{Ledger, frame, identity, items, raw, turns};

#[test]
fn history_metadata_never_hydrates_nested_turns_or_defaults_unknown_fields() -> anyhow::Result<()> {
    let request = HistoryQuery::metadata(identity()?, 2)?.request(RequestId::Integer(7));
    let observation = frame(
        json!({"thread":{"id":"Thread:保留 ","turns":[{"id":"nested", "items":[{"id":"hidden"}]}],"extra":"retained"}}),
    )?;
    let page = HistoryPage::decode(&observation, &request)?;
    let metadata = page
        .metadata()
        .ok_or_else(|| anyhow::anyhow!("missing metadata"))?;
    assert_eq!(metadata.history_mode, Field::Missing);
    assert_eq!(metadata.ephemeral, Field::Missing);
    assert_eq!(metadata.evidence.json_pointer, "/result/thread");
    assert_eq!(metadata.provider_payload["extra"], "retained");
    assert_eq!(page.entries().count(), 0);
    let step = next_history_step(&page, &Ledger::default());
    assert!(step.coverage.uncertain_metadata);
    let HistoryAction::ReadTurns(query) = step.action else {
        anyhow::bail!("expected turns query");
    };
    assert_eq!(
        query.request(RequestId::Integer(8)).wire_value()["params"]["itemsView"],
        "notLoaded"
    );
    Ok(())
}

#[test]
fn history_turns_keep_views_seconds_and_original_entry_pointers() -> anyhow::Result<()> {
    let request = turns(None)?;
    let observation = frame(json!({"data":[
        {"id":"Turn:1", "itemsView":"notLoaded", "startedAt":10, "status":"completed", "items":[]},
        {"id":"Turn:2", "itemsView":"summary", "completedAt":11, "status":"completed", "items":[{"id":"retained"}]},
        {"id":"Turn:3", "itemsView":"full", "status":"completed", "items":[]},
        {"id":"Turn:4", "status":"completed", "items":[]},
        {"id":"Turn:5", "itemsView":"future", "status":"future", "items":[]}
    ],"nextCursor":null,"backwardsCursor":" reverse\u{0}"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    let entries = page.entries().collect::<Result<Vec<_>, _>>()?;
    for (index, entry) in entries.iter().enumerate() {
        let HistoryEntry::Turn(turn) = entry else {
            anyhow::bail!("expected turn");
        };
        assert_eq!(turn.evidence.json_pointer, format!("/result/data/{index}"));
        assert_eq!(turn.evidence.origin, EvidenceOrigin::RpcResponse);
    }
    let HistoryEntry::Turn(first) = &entries[0] else {
        anyhow::bail!("expected turn");
    };
    assert_eq!(first.started_at_seconds, Field::Value(10));
    let HistoryEntry::Turn(missing) = &entries[3] else {
        anyhow::bail!("expected turn");
    };
    assert_eq!(missing.items_view, Field::Missing);
    assert_eq!(page.backwards_cursor(), &Field::Value(" reverse\0".into()));
    let step = next_history_step(&page, &Ledger::default());
    assert_eq!(step.action, HistoryAction::ScopeEndObserved);
    assert_eq!(step.coverage.uncertain_entry_semantics, 2);
    assert_eq!(page.raw_payload(), observation.raw_payload);
    Ok(())
}

#[test]
fn history_items_preserve_unknown_payload_timing_and_valid_siblings() -> anyhow::Result<()> {
    let request = items(None)?;
    let observation = frame(json!({"data":[
        {"turnId":"Turn:A","startedAtMs":1001,"completedAtMs":1009,"item":{"id":"one","type":"agentMessage","text":"whole\u{0}text"}},
        {"turnId":"Turn:A","item":{"id":"future","type":"futureType","unknown":{"nested":true}}},
        {"turnId":"other","item":{"id":"foreign","type":"agentMessage"}},
        {"turnId":"Turn:A","item":{"id":"","type":"agentMessage"}},
        15,
        {"turnId":"Turn:A","item":{"id":"last","type":"agentMessage"}}
    ],"nextCursor":null,"extra":"outer"}))?;
    let page = HistoryPage::decode(&observation, &request)?;
    let entries = page.entries().collect::<Vec<_>>();
    let HistoryEntry::Item(first) = entries[0].as_ref().map_err(|e| anyhow::anyhow!("{e}"))? else {
        anyhow::bail!("expected item");
    };
    assert_eq!(first.started_at_ms, Field::Value(1001));
    assert_eq!(first.completed_at_ms, Field::Value(1009));
    assert_eq!(first.evidence.json_pointer, "/result/data/0/item");
    assert_eq!(first.provider_payload["text"], "whole\0text");
    assert_eq!(first.entry_payload["startedAtMs"], 1001);
    let HistoryEntry::Item(future) = entries[1].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?
    else {
        anyhow::bail!("expected item");
    };
    assert_eq!(future.category, ItemCategory::Unknown);
    assert_eq!(future.provider_payload["unknown"]["nested"], true);
    assert!(entries[2].is_err());
    assert!(entries[3].is_err());
    assert!(entries[4].is_err());
    assert!(entries[5].is_ok());
    let step = next_history_step(&page, &Ledger::default());
    assert_eq!(step.coverage.undiscovered_entries, 3);
    assert_eq!(step.coverage.uncertain_entry_semantics, 1);
    assert_eq!(page.provider_result()["extra"], "outer");
    Ok(())
}

#[test]
fn history_duplicate_entry_identity_is_partial_and_content_duplicates_stay_raw()
-> anyhow::Result<()> {
    let bytes = br#" {"id":7,"result":{"data":[{"turnId":"Turn:A","item":{"id":"old","id":"new","type":"agentMessage"}},{"turnId":"Turn:A","item":{"id":"ok","type":"agentMessage","text":"before","text":"after"}}],"nextCursor":null}} "#;
    let request = items(None)?;
    let observation = raw(bytes)?;
    let page = HistoryPage::decode(&observation, &request)?;
    let mut entries = page.entries();
    let first = entries
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing first entry"))?;
    assert_eq!(
        first.map_err(|e| e.kind),
        Err(HistoryErrorKind::DuplicateControlField)
    );
    let second = entries
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing second entry"))??;
    let HistoryEntry::Item(item) = second else {
        anyhow::bail!("expected item");
    };
    assert_eq!(item.item_id, "ok");
    assert_eq!(page.raw_payload(), bytes);
    assert_eq!(
        next_history_step(&page, &Ledger::default())
            .coverage
            .undiscovered_entries,
        1
    );
    Ok(())
}
