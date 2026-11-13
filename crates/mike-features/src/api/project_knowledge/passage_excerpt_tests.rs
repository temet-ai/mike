use serde_json::json;

use super::*;

fn passages(reference: &str, text: &str) -> Passages {
    Passages::from([(reference.to_owned(), text.to_owned())])
}

#[test]
fn show_reranker_passages_item_with_a_passage_gets_it_and_the_marker() {
    let mut items = vec![json!({"reference": "note:1", "excerpt": "old", "identifier_hits": 0})];

    show_reranker_passages(&mut items, &passages("note:1", "the matching chunk"));

    assert_eq!(items[0]["excerpt"], "the matching chunk");
    assert_eq!(items[0]["excerpt_from"], "passage");
}

#[test]
fn show_reranker_passages_item_with_identifier_hits_keeps_its_excerpt() {
    let mut items =
        vec![json!({"reference": "note:1", "excerpt": "around #123", "identifier_hits": 2})];

    show_reranker_passages(&mut items, &passages("note:1", "the matching chunk"));

    assert_eq!(items[0]["excerpt"], "around #123");
    assert!(items[0].get("excerpt_from").is_none());
}

#[test]
fn show_reranker_passages_item_without_a_passage_keeps_its_excerpt() {
    let mut items = vec![json!({"reference": "note:2", "excerpt": "old"})];

    show_reranker_passages(&mut items, &passages("note:1", "other"));

    assert_eq!(items[0]["excerpt"], "old");
    assert!(items[0].get("excerpt_from").is_none());
}

#[test]
fn show_reranker_passages_long_passage_is_cut_to_what_the_reranker_reads() {
    let long = "é".repeat(MAX_DOCUMENT_CHARS + 50);
    let mut items = vec![json!({"reference": "note:1", "excerpt": "old"})];

    show_reranker_passages(&mut items, &passages("note:1", &long));

    let shown = items[0]["excerpt"]
        .as_str()
        .map_or(0, |text| text.chars().count());
    assert_eq!(shown, MAX_DOCUMENT_CHARS);
}

#[test]
fn show_reranker_passages_blank_passage_keeps_the_excerpt() {
    let mut items = vec![json!({"reference": "note:1", "excerpt": "old"})];

    show_reranker_passages(&mut items, &passages("note:1", "   \n"));

    assert_eq!(items[0]["excerpt"], "old");
}
