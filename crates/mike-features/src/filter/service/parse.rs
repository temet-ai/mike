use super::super::{FilterAction, FilterKind};

pub fn parse_kind(raw: &str) -> Result<FilterKind, String> {
    match raw {
        "text_substring" => Ok(FilterKind::TextSubstring),
        "xml_tag" => Ok(FilterKind::XmlTag),
        "lazy_section" => Ok(FilterKind::LazySection),
        "inject_reminder" => Ok(FilterKind::InjectReminder),
        other => Err(format!(
            "unknown kind `{other}` — expected `text_substring`, `xml_tag`, \
             `lazy_section`, or `inject_reminder`"
        )),
    }
}

pub fn parse_action(raw: &str) -> Result<FilterAction, String> {
    match raw {
        "drop" => Ok(FilterAction::Drop),
        "replace" => Ok(FilterAction::Replace),
        "append" => Ok(FilterAction::Append),
        other => Err(format!(
            "unknown action `{other}` — expected `drop`, `replace`, or `append`"
        )),
    }
}
