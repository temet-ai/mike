use super::style_check;
use super::types::{FilterAction, FilterKind, FilterRule};

impl FilterRule {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_structure()?;
        self.validate_style()
    }

    pub fn validate_structure(&self) -> Result<(), String> {
        if self.kind == FilterKind::LazySection {
            return self.validate_lazy_section();
        }
        if self.kind == FilterKind::InjectReminder {
            return self.validate_inject_reminder();
        }
        if self.trigger_token.is_some() {
            return Err(format!(
                "filter rule {}: `trigger_token` is only valid on kind=lazy_section or kind=inject_reminder",
                self.id
            ));
        }
        match self.action {
            FilterAction::Drop => {
                if self.text.is_some() {
                    return Err(format!(
                        "filter rule {}: action=drop must not have a `text` field",
                        self.id
                    ));
                }
            }
            FilterAction::Replace | FilterAction::Append => {
                let txt = self.text.as_ref().ok_or_else(|| {
                    format!(
                        "filter rule {}: action={} requires a non-empty `text` field",
                        self.id,
                        action_name(self.action),
                    )
                })?;
                if txt.is_empty() {
                    return Err(format!(
                        "filter rule {}: action={} requires a non-empty `text` field",
                        self.id,
                        action_name(self.action),
                    ));
                }
                reject_unsafe_codepoints(self.id, "text", txt)?;
            }
        }
        if self.value.is_empty() {
            return Err(format!(
                "filter rule {}: `value` must not be empty",
                self.id
            ));
        }
        reject_unsafe_codepoints(self.id, "value", &self.value)?;
        if self.kind == FilterKind::XmlTag {
            reject_invalid_xml_element_name(self.id, &self.value)?;
        }
        Ok(())
    }

    fn validate_style(&self) -> Result<(), String> {
        if self.style_override {
            return Ok(());
        }
        let Some(banned) = style_check::load_banned_terms() else {
            return Ok(());
        };
        for (field, value) in [
            ("text", self.text.as_deref()),
            ("inject_text", self.inject_text.as_deref()),
        ] {
            let Some(text) = value else { continue };
            if let Some(phrase) = style_check::find_banned_term(text, &banned) {
                return Err(format!(
                    "filter rule {}: `{field}` contains vocabulary banned by \
                     ~/.claude/rules/response-style.md: {phrase:?} (set style_override=true \
                     to save anyway)",
                    self.id
                ));
            }
        }
        Ok(())
    }

    fn validate_lazy_section(&self) -> Result<(), String> {
        if self.action != FilterAction::Replace {
            return Err(format!(
                "filter rule {}: kind=lazy_section requires action=replace \
                 (the heading span is replaced by the stub)",
                self.id
            ));
        }
        let txt = self
            .text
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                format!(
                    "filter rule {}: kind=lazy_section requires a non-empty `text` \
                     field (the stub that replaces the stripped section)",
                    self.id
                )
            })?;
        reject_unsafe_codepoints(self.id, "text", txt)?;
        let token = self
            .trigger_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                format!(
                    "filter rule {}: kind=lazy_section requires a non-empty \
                     `trigger_token` field",
                    self.id
                )
            })?;
        reject_unsafe_codepoints(self.id, "trigger_token", token)?;
        if self.value.is_empty() {
            return Err(format!(
                "filter rule {}: `value` must not be empty",
                self.id
            ));
        }
        reject_unsafe_codepoints(self.id, "value", &self.value)?;
        Ok(())
    }

    fn validate_inject_reminder(&self) -> Result<(), String> {
        if self.action != FilterAction::Replace {
            return Err(format!(
                "filter rule {}: kind=inject_reminder requires action=replace \
                 (the action is not user-meaningful; it is pinned to replace)",
                self.id
            ));
        }
        if self.value.is_empty() {
            return Err(format!(
                "filter rule {}: kind=inject_reminder requires a non-empty `value` \
                 (the operator label)",
                self.id
            ));
        }
        reject_unsafe_codepoints(self.id, "value", &self.value)?;
        let txt = self
            .text
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                format!(
                    "filter rule {}: kind=inject_reminder requires a non-empty `text` \
                     field (the reminder body to inject)",
                    self.id
                )
            })?;
        reject_unsafe_codepoints(self.id, "text", txt)?;
        let token = self
            .trigger_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                format!(
                    "filter rule {}: kind=inject_reminder requires a non-empty \
                     `trigger_token` field (the marker)",
                    self.id
                )
            })?;
        reject_unsafe_codepoints(self.id, "trigger_token", token)?;
        Ok(())
    }
}

fn action_name(a: FilterAction) -> &'static str {
    match a {
        FilterAction::Drop => "drop",
        FilterAction::Replace => "replace",
        FilterAction::Append => "append",
    }
}

fn reject_unsafe_codepoints(rule_id: i64, field: &str, s: &str) -> Result<(), String> {
    for ch in s.chars() {
        let bad = match ch {
            '\t' | '\n' | '\r' => false,
            '\x00'..='\x1f' => true,
            '\u{202A}'..='\u{202E}' => true,
            _ => false,
        };
        if bad {
            return Err(format!(
                "filter rule {rule_id}: `{field}` contains invalid character U+{code:04X} \
                 (C0 controls and bidi overrides are rejected)",
                code = ch as u32,
            ));
        }
    }
    Ok(())
}

fn reject_invalid_xml_element_name(rule_id: i64, value: &str) -> Result<(), String> {
    fn is_name_start(c: char) -> bool {
        c.is_ascii_alphabetic() || c == '_' || c == ':'
    }
    fn is_name_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '-'
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if is_name_start(c) => {}
        _ => {
            return Err(format!(
                "filter rule {rule_id}: xml_tag `value` must start with a letter, `_`, or `:` \
                 (got {value:?})"
            ));
        }
    }
    for c in chars {
        if !is_name_char(c) {
            return Err(format!(
                "filter rule {rule_id}: xml_tag `value` contains invalid character {c:?} \
                 (only letters, digits, `_`, `:`, `.`, `-` allowed; got {value:?})"
            ));
        }
    }
    Ok(())
}
