use crate::store::persistence::messages::{
    CanonicalMessage, SentPatchRow, apply_sent_patches, canonical_clean_messages,
};

use super::derive::{
    MessageKey, plan_sent_messages, sent_envelope, sent_envelope_candidate, sent_messages_split,
    whole_sent_envelope,
};
use super::store::BridgedMessage;

pub(crate) struct LegacyRow<'a> {
    pub raw_body: &'a [u8],
    pub request_envelope: Option<&'a [u8]>,
    pub patches: Vec<SentPatchRow>,
    pub bridged: &'a [BridgedMessage],
}

#[derive(Debug)]
pub(crate) struct DerivedSent {
    pub true_sent: Vec<u8>,
    pub envelope: Option<Vec<u8>>,
    pub count: Option<i32>,
    pub rows: Vec<CanonicalMessage>,
}

pub(crate) fn derive_from_legacy(row: LegacyRow<'_>) -> Result<DerivedSent, String> {
    let true_sent = apply_sent_patches(row.raw_body, row.patches)
        .map_err(|e| format!("old patches do not apply to request_body: {e}"))?;
    let mut derived = DerivedSent {
        envelope: sent_envelope(row.raw_body, row.request_envelope, &true_sent),
        count: None,
        rows: Vec::new(),
        true_sent: Vec::new(),
    };
    if row.raw_body != true_sent.as_slice() && sent_messages_split(&true_sent) {
        match canonical_clean_messages(&true_sent) {
            Ok(sent) => {
                let keys: Vec<MessageKey<'_>> = row
                    .bridged
                    .iter()
                    .map(|m| MessageKey {
                        sha: &m.sha,
                        cache_control: m.cache_control.as_ref(),
                    })
                    .collect();
                let plan = plan_sent_messages(&keys, sent);
                derived.count = plan.count;
                derived.rows = plan.rows;
            }
            Err(_) => derived.envelope = Some(whole_sent_envelope(row.raw_body, &true_sent)),
        }
    }
    if derived.envelope.is_none() && derived.count.is_none() && derived.rows.is_empty() {
        derived.envelope = Some(sent_envelope_candidate(row.raw_body, &true_sent));
    }
    derived.true_sent = true_sent;
    Ok(derived)
}
