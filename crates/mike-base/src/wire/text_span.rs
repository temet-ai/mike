use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub origin: String,
    pub start: usize,
    pub end: usize,
    pub first_request_id: i64,
    pub last_request_id: i64,
}

impl TextSpan {
    pub fn is_sidecar(&self) -> bool {
        self.origin.ends_with("_sidecar")
    }
}

pub fn parse_spans(value: &Value) -> Vec<TextSpan> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            Some(TextSpan {
                origin: item.get("origin")?.as_str()?.to_string(),
                start: usize::try_from(item.get("start")?.as_i64()?).ok()?,
                end: usize::try_from(item.get("end")?.as_i64()?).ok()?,
                first_request_id: item.get("first_request_id")?.as_i64()?,
                last_request_id: item.get("last_request_id")?.as_i64()?,
            })
        })
        .collect()
}
