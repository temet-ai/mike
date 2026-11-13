pub mod injector;
pub mod matcher;
pub mod stage;

use bytes::Bytes;
use serde_json::Value;

use crate::filter::{FilterConfig, FilterKind, FilterRule, project_scope_matches};

#[derive(Debug)]
pub enum LazySectionsOutcome {
    Unchanged,
    Rewrote {
        new_body: Bytes,
        original_len: usize,
        rewritten_len: usize,
        stripped_bytes: usize,
        injected: bool,
        firing_rule_ids: Vec<i64>,
    },
}

fn active_lazy_rules<'a>(cfg: &'a FilterConfig, project_path: &str) -> Vec<&'a FilterRule> {
    cfg.rules
        .iter()
        .filter(|r| {
            matches!(r.kind, FilterKind::LazySection | FilterKind::InjectReminder)
                && r.enabled
                && r.project_path
                    .as_deref()
                    .is_none_or(|p| project_scope_matches(p, project_path))
        })
        .collect()
}

pub fn has_active_rules(cfg: &FilterConfig, project_path: &str) -> bool {
    !active_lazy_rules(cfg, project_path).is_empty()
}

#[derive(Default)]
struct RewriteTally {
    stripped_bytes: usize,
    injected: bool,
    firing_rule_ids: Vec<i64>,
}

impl RewriteTally {
    fn record_firing(&mut self, rule_id: i64) {
        if !self.firing_rule_ids.contains(&rule_id) {
            self.firing_rule_ids.push(rule_id);
        }
    }
}

fn reminder_body_for_rule(
    rule: &FilterRule,
    value: &mut Value,
    text: &str,
    tally: &mut RewriteTally,
) -> Option<String> {
    match rule.kind {
        FilterKind::LazySection => {
            let section_text = strip_section(value, &rule.value, text)?;
            tally.stripped_bytes += section_text.len();
            tally.record_firing(rule.id);
            Some(rule.inject_text.clone().unwrap_or(section_text))
        }
        FilterKind::InjectReminder => Some(text.to_string()),
        FilterKind::TextSubstring | FilterKind::XmlTag => None,
    }
}

fn apply_rule(rule: &FilterRule, value: &mut Value, tally: &mut RewriteTally) {
    let (Some(text), Some(token)) = (rule.text.as_deref(), rule.trigger_token.as_deref()) else {
        return;
    };
    let Some(reminder_body) = reminder_body_for_rule(rule, value, text, tally) else {
        return;
    };
    let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    let reminder = wrap_reminder(token, &reminder_body);
    if injector::inject_after_trigger(messages, token, &reminder) {
        tally.injected = true;
        tally.record_firing(rule.id);
    }
}

pub fn apply_lazy_sections(
    raw: &[u8],
    cfg: &FilterConfig,
    project_path: &str,
) -> LazySectionsOutcome {
    let active = active_lazy_rules(cfg, project_path);
    if active.is_empty() {
        return LazySectionsOutcome::Unchanged;
    }

    let mut value: Value = match serde_json::from_slice(raw) {
        Ok(v) => v,
        Err(_) => return LazySectionsOutcome::Unchanged,
    };
    if !value.is_object() {
        return LazySectionsOutcome::Unchanged;
    }

    if has_thinking_blocks(&value) {
        tracing::debug!(
            "lazy sections skipped: thinking blocks present in messages (R1 signature guard)"
        );
        return LazySectionsOutcome::Unchanged;
    }

    let mut tally = RewriteTally::default();
    for rule in active {
        apply_rule(rule, &mut value, &mut tally);
    }

    if tally.firing_rule_ids.is_empty() {
        return LazySectionsOutcome::Unchanged;
    }

    let new_bytes = match serde_json::to_vec(&value) {
        Ok(b) => b,
        Err(_) => return LazySectionsOutcome::Unchanged,
    };
    let rewritten_len = new_bytes.len();
    LazySectionsOutcome::Rewrote {
        new_body: Bytes::from(new_bytes),
        original_len: raw.len(),
        rewritten_len,
        stripped_bytes: tally.stripped_bytes,
        injected: tally.injected,
        firing_rule_ids: tally.firing_rule_ids,
    }
}

fn strip_section(body: &mut Value, heading: &str, stub: &str) -> Option<String> {
    let system = body.get_mut("system")?.as_array_mut()?;
    let global_boundary = crate::cache_scope::last_global_index(system);
    for (idx, block) in system.iter_mut().enumerate() {
        if global_boundary.is_some_and(|boundary| idx <= boundary) {
            continue;
        }
        let obj = block.as_object_mut()?;
        if obj.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = obj.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Some((start, end)) = matcher::find_heading_span(text, heading) else {
            continue;
        };
        let section_text = text[start..end].to_string();
        let mut new_text = String::with_capacity(text.len() - section_text.len() + stub.len() + 1);
        new_text.push_str(&text[..start]);
        new_text.push_str(stub);
        if end < text.len() && !new_text.ends_with('\n') {
            new_text.push('\n');
        }
        new_text.push_str(&text[end..]);
        if new_text.trim().is_empty() {
            system.remove(idx);
        } else {
            obj.insert("text".to_string(), Value::String(new_text));
        }
        return Some(section_text);
    }
    None
}

fn wrap_reminder(token: &str, body: &str) -> String {
    match bare_tag_name(token) {
        Some(tag) => format!("<{tag}>\n{body}\n</{tag}>"),
        None => format!("<system-reminder>\n{body}\n</system-reminder>"),
    }
}

fn bare_tag_name(token: &str) -> Option<&str> {
    let inner = token.trim().strip_prefix('<')?.strip_suffix('>')?;
    let ok = !inner.is_empty()
        && inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(inner)
}

pub fn has_thinking_blocks(body: &Value) -> bool {
    body.get("messages")
        .and_then(Value::as_array)
        .is_some_and(|msgs| {
            msgs.iter().any(|m| {
                m.get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|arr| {
                        arr.iter().any(|b| {
                            matches!(
                                b.get("type").and_then(Value::as_str),
                                Some("thinking") | Some("redacted_thinking")
                            )
                        })
                    })
            })
        })
}

#[cfg(test)]
pub mod tests_support;

#[cfg(test)]
pub mod tests_lazy_section;

#[cfg(test)]
pub mod tests_inject_reminder;
