use serde::Deserialize;
use serde_json::value::RawValue;

use super::policy::BodyCapture;

pub(super) const DUPLICATED_PAYLOAD_TYPES: &[&str] =
    &["stream_event", "user", "assistant", "message"];

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RequestPlan<'a> {
    pub(crate) capture: BodyCapture,
    pub(crate) payloads: Vec<ClassifiedPayload<'a>>,
    pub(crate) payload_link: Option<PayloadSessionLink>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ClassifiedPayload<'a> {
    pub(crate) ordinal: i32,
    pub(crate) payload_type: &'a str,
    pub(crate) body: Option<&'a [u8]>,
    pub(crate) body_sha256: [u8; 32],
    pub(crate) body_size: i32,
}

pub(super) fn classify_payloads(body: &[u8]) -> Option<Vec<ClassifiedPayload<'_>>> {
    let items = payload_items(body)?;
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let head: PayloadItem = serde_json::from_str(item.get()).ok()?;
            let bytes = item.get().as_bytes();
            let duplicated = DUPLICATED_PAYLOAD_TYPES.contains(&head.payload.payload_type);
            Some(ClassifiedPayload {
                ordinal: i32::try_from(index).unwrap_or(i32::MAX),
                payload_type: head.payload.payload_type,
                body: (!duplicated).then_some(bytes),
                body_sha256: <sha2::Sha256 as sha2::Digest>::digest(bytes).into(),
                body_size: i32::try_from(bytes.len()).unwrap_or(i32::MAX),
            })
        })
        .collect()
}

fn payload_items(body: &[u8]) -> Option<Vec<&RawValue>> {
    if let Some(items) = as_payload_items(body) {
        return Some(items);
    }
    let top: std::collections::BTreeMap<&str, &RawValue> = serde_json::from_slice(body).ok()?;
    if top.contains_key("payload") {
        let whole: &RawValue = serde_json::from_slice(body).ok()?;
        return Some(vec![whole]);
    }
    top.values()
        .find_map(|field| as_payload_items(field.get().as_bytes()))
}

fn as_payload_items(raw: &[u8]) -> Option<Vec<&RawValue>> {
    let items: Vec<&RawValue> = serde_json::from_slice(raw).ok()?;
    if items.is_empty() {
        return None;
    }
    items
        .iter()
        .all(|item| serde_json::from_str::<PayloadItem>(item.get()).is_ok())
        .then_some(items)
}

#[derive(Debug, Deserialize)]
struct PayloadItem<'a> {
    #[serde(borrow)]
    payload: PayloadHead<'a>,
}

#[derive(Debug, Deserialize)]
struct PayloadHead<'a> {
    #[serde(rename = "type", borrow)]
    payload_type: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PayloadSessionLink {
    pub(crate) session_uuid: String,
    pub(crate) cwd: Option<String>,
    pub(crate) client_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionLinkItem<'a> {
    #[serde(borrow)]
    payload: SessionLinkFields<'a>,
}

#[derive(Debug, Deserialize)]
struct SessionLinkFields<'a> {
    #[serde(default, rename = "sessionId", borrow)]
    session_id: Option<&'a str>,
    #[serde(default, borrow)]
    cwd: Option<&'a str>,
    #[serde(default, borrow)]
    version: Option<&'a str>,
}

pub(crate) fn payload_session_link(payload_type: &str, body: &[u8]) -> Option<PayloadSessionLink> {
    if payload_type != "attachment" && payload_type != "system" {
        return None;
    }
    let item: SessionLinkItem<'_> = serde_json::from_slice(body).ok()?;
    Some(PayloadSessionLink {
        session_uuid: item.payload.session_id?.to_owned(),
        cwd: item.payload.cwd.map(str::to_owned),
        client_version: item.payload.version.map(str::to_owned),
    })
}
