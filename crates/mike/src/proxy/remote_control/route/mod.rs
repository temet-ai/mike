mod payload;
mod policy;
mod signals;
mod stored_body;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "link_tests.rs"]
mod link_tests;

pub(crate) use payload::{ClassifiedPayload, PayloadSessionLink};
pub(crate) use policy::endpoint_labels;
pub(crate) use signals::ChannelSignals;
pub(crate) use stored_body::stored_body;

const PATH_PREFIX: &str = "/v1/code/sessions/";

const ID_PREFIX: &str = "cse_";

const MAX_CSE_ID_LEN: usize = 64;

const MAX_SUFFIX_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteControlRoute<'a> {
    pub(crate) cse_id: &'a str,
    pub(crate) suffix: &'a str,
}

pub(crate) fn recognise(path: &str) -> Option<RemoteControlRoute<'_>> {
    let rest = path.strip_prefix(PATH_PREFIX)?;
    let (cse_id, suffix) = match rest.find('/') {
        Some(boundary) => rest.split_at(boundary),
        None => (rest, ""),
    };
    if !is_channel_id(cse_id) || suffix.len() > MAX_SUFFIX_LEN {
        return None;
    }
    Some(RemoteControlRoute { cse_id, suffix })
}

pub(crate) fn is_channel_id(segment: &str) -> bool {
    let Some(body) = segment.strip_prefix(ID_PREFIX) else {
        return false;
    };
    segment.len() <= MAX_CSE_ID_LEN
        && !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_alphanumeric())
}
