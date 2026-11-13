use bytes::Bytes;

use super::appender::append_block_to_last_user_message;
use crate::store::models::{RequestId, SystemReminder};

pub struct InjectBookkeeping<'a> {
    pub post_filter_body: &'a mut Bytes,
    pub filtered_stored: &'a mut Option<Bytes>,
    pub filter_applied: &'a mut bool,
    pub request_id: RequestId,
}

pub fn inject_system_reminder(reminder: &SystemReminder, bk: &mut InjectBookkeeping<'_>) -> bool {
    match append_block_to_last_user_message(bk.post_filter_body, &reminder.payload, bk.request_id) {
        Some(new_bytes) => {
            let new_bytes = Bytes::from(new_bytes);
            crate::pipeline::absorb_body_rewrite(
                bk.post_filter_body,
                bk.filtered_stored,
                bk.filter_applied,
                new_bytes,
            );
            tracing::info!(
                source = ?reminder.source,
                payload_len = reminder.payload.len(),
                "inject_system_reminder: appended system-reminder block"
            );
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::models::{ReminderSource, SystemReminder};
    use serde_json::{Value, json};

    fn user_body() -> Bytes {
        Bytes::from_static(
            br#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":[{"type":"text","text":"q"}]}]}"#,
        )
    }

    fn make_reminder(payload: &str) -> SystemReminder {
        SystemReminder {
            source: ReminderSource::Diagnostic,
            payload: payload.to_string(),
        }
    }

    #[test]
    fn inject_appends_and_updates_bookkeeping() -> anyhow::Result<()> {
        let mut body = user_body();
        let mut stored: Option<Bytes> = None;
        let mut applied = false;
        let reminder = make_reminder("<system-reminder>do the thing</system-reminder>");
        let result = inject_system_reminder(
            &reminder,
            &mut InjectBookkeeping {
                post_filter_body: &mut body,
                filtered_stored: &mut stored,
                filter_applied: &mut applied,
                request_id: RequestId::next(),
            },
        );
        assert!(result, "expected inject to succeed on a valid user body");
        assert!(applied);
        assert!(stored.is_some());
        let new_val: Value = serde_json::from_slice(&body)?;
        let last_block = &new_val["messages"][0]["content"][1];
        assert_eq!(last_block["type"], "text");
        assert_eq!(
            last_block["text"],
            "<system-reminder>do the thing</system-reminder>"
        );
        Ok(())
    }

    #[test]
    fn inject_returns_false_for_assistant_last_turn() -> anyhow::Result<()> {
        let body_val = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "q"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "a"}]},
            ],
        });
        let mut body = Bytes::from(serde_json::to_vec(&body_val)?);
        let original = body.clone();
        let mut stored: Option<Bytes> = None;
        let mut applied = false;
        let reminder = make_reminder("X");
        let result = inject_system_reminder(
            &reminder,
            &mut InjectBookkeeping {
                post_filter_body: &mut body,
                filtered_stored: &mut stored,
                filter_applied: &mut applied,
                request_id: RequestId::next(),
            },
        );
        assert!(!result, "assistant last turn must not be injected into");
        assert!(!applied, "filter_applied must stay false");
        assert!(stored.is_none(), "filtered_stored must stay None");
        assert_eq!(body, original, "body must be unchanged");
        Ok(())
    }

    #[test]
    fn inject_does_not_overwrite_existing_filtered_stored() -> anyhow::Result<()> {
        let mut body = user_body();
        let mut stored: Option<Bytes> = None;
        let mut applied = false;

        inject_system_reminder(
            &make_reminder("first"),
            &mut InjectBookkeeping {
                post_filter_body: &mut body,
                filtered_stored: &mut stored,
                filter_applied: &mut applied,
                request_id: RequestId::next(),
            },
        );
        let after_first = body.clone();

        inject_system_reminder(
            &make_reminder("second"),
            &mut InjectBookkeeping {
                post_filter_body: &mut body,
                filtered_stored: &mut stored,
                filter_applied: &mut applied,
                request_id: RequestId::next(),
            },
        );
        assert_ne!(
            body, after_first,
            "second inject should have changed the body"
        );
        assert_eq!(
            stored
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("stored is Some after a successful inject"))?,
            &body
        );
        Ok(())
    }
}
