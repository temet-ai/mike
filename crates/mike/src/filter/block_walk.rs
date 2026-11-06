use serde_json::{Map, Value};
use tracing::warn;

use super::content_blocks::is_text_block;
use super::matcher::{block_matches, find_text_substring};
use super::{FilterAction, FilterKind, FilterRule};

pub(super) fn walk_blocks(
    arr: &mut Vec<Value>,
    rules: &[&FilterRule],
    firing_rule_ids: &mut Vec<i64>,
    protect_global_prefix: bool,
) -> bool {
    let mut new_blocks: Vec<Value> = Vec::with_capacity(arr.len());
    let mut changed = false;

    let original = std::mem::take(arr);
    let global_boundary = protect_global_prefix
        .then(|| crate::cache_scope::last_global_index(&original))
        .flatten();

    for (idx, block) in original.into_iter().enumerate() {
        if global_boundary.is_some_and(|boundary| idx <= boundary) {
            new_blocks.push(block);
            continue;
        }
        if !is_text_block(&block) {
            new_blocks.push(block);
            continue;
        }

        let text = block
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let transform = apply_rules_to_text(&text, rules, firing_rule_ids);
        changed |=
            transform.dropped || transform.text != text || !transform.appended_blocks.is_empty();

        if !transform.dropped {
            if transform.text == text {
                new_blocks.push(block);
            } else {
                let mut updated = block;
                if let Value::Object(ref mut obj) = updated {
                    obj.insert("text".to_string(), Value::String(transform.text));
                }
                new_blocks.push(updated);
            }
        }
        for appended_text in transform.appended_blocks {
            let mut appended = Map::new();
            appended.insert("type".to_string(), Value::String("text".to_string()));
            appended.insert("text".to_string(), Value::String(appended_text));
            new_blocks.push(Value::Object(appended));
        }
    }

    *arr = new_blocks;
    changed
}

enum MatchSpan {
    Whole,
    Slice(usize, usize),
}

struct BlockTransform {
    text: String,
    dropped: bool,
    appended_blocks: Vec<String>,
}

fn apply_rules_to_text(
    original: &str,
    rules: &[&FilterRule],
    firing_rule_ids: &mut Vec<i64>,
) -> BlockTransform {
    let mut text = original.to_string();
    let mut appended_blocks: Vec<String> = Vec::new();
    let mut dropped = false;

    for rule in rules {
        let span = match rule.kind {
            FilterKind::XmlTag => block_matches(rule, &text).then_some(MatchSpan::Whole),
            FilterKind::TextSubstring => {
                find_text_substring(&text, &rule.value).map(|(start, end)| {
                    if start == 0 && end == text.len() {
                        MatchSpan::Whole
                    } else {
                        MatchSpan::Slice(start, end)
                    }
                })
            }
            FilterKind::LazySection | FilterKind::InjectReminder => None,
        };
        let Some(span) = span else {
            continue;
        };

        if !firing_rule_ids.contains(&rule.id) {
            firing_rule_ids.push(rule.id);
        }

        match (span, rule.action) {
            (MatchSpan::Whole, FilterAction::Drop) => {
                dropped = true;
                break;
            }
            (MatchSpan::Whole, FilterAction::Replace) => match rule.text {
                Some(ref t) => {
                    text = t.clone();
                    break;
                }
                None => warn_missing_text(rule, "replace"),
            },
            (MatchSpan::Whole, FilterAction::Append) => match rule.text {
                Some(ref t) => appended_blocks.push(t.clone()),
                None => warn_missing_text(rule, "append"),
            },
            (MatchSpan::Slice(start, end), FilterAction::Drop) => {
                text.replace_range(start..end, "");
            }
            (MatchSpan::Slice(start, end), FilterAction::Replace) => match rule.text {
                Some(ref t) => text.replace_range(start..end, t),
                None => warn_missing_text(rule, "replace"),
            },
            (MatchSpan::Slice(_, end), FilterAction::Append) => match rule.text {
                Some(ref t) => text.insert_str(end, t),
                None => warn_missing_text(rule, "append"),
            },
        }
    }

    if !dropped && text != original && text.trim().is_empty() {
        dropped = true;
    }

    BlockTransform {
        text,
        dropped,
        appended_blocks,
    }
}

fn warn_missing_text(rule: &FilterRule, action: &'static str) {
    warn!(
        rule_id = %rule.id,
        action,
        "filter rule invariant violated: action requires text but text is None; skipping block transformation"
    );
}
