use bytes::Bytes;
use serde_json::Value;

use super::frames::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StitchPoint {
    pub open_block: Option<u64>,
    pub next_index: u64,
}

pub fn onto_the_first_reply(frame: Frame, next_index: u64) -> Option<Bytes> {
    match frame.event.as_str() {
        "message_start" => None,
        "content_block_start" | "content_block_delta" | "content_block_stop" if next_index > 0 => {
            Some(reindexed(&frame, next_index).unwrap_or(frame.raw))
        }
        _ => Some(frame.raw),
    }
}

fn reindexed(frame: &Frame, next_index: u64) -> Option<Bytes> {
    let mut json: Value = serde_json::from_str(&frame.data).ok()?;
    let index = json.get("index")?.as_u64()?;
    json.as_object_mut()?
        .insert("index".to_owned(), Value::from(index + next_index));
    Some(sse_frame(&frame.event, &json.to_string()))
}

pub fn block_stop_frame(index: u64) -> Bytes {
    sse_frame(
        "content_block_stop",
        &serde_json::json!({"type": "content_block_stop", "index": index}).to_string(),
    )
}

pub fn error_frame(message: &str) -> Bytes {
    sse_frame(
        "error",
        &serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": message}
        })
        .to_string(),
    )
}

pub fn upstream_error_frame(status: u16, body: &[u8]) -> Bytes {
    match serde_json::from_slice::<Value>(body) {
        Ok(json) if json.get("type").and_then(Value::as_str) == Some("error") => {
            sse_frame("error", &json.to_string())
        }
        _ => error_frame(&format!(
            "the mike proxy asked upstream for the rest of this reply and got status {status}"
        )),
    }
}

fn sse_frame(event: &str, data: &str) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn frame(event: &str, data: &str) -> Frame {
        Frame {
            raw: sse_frame(event, data),
            event: event.to_owned(),
            data: data.to_owned(),
        }
    }

    fn data_of(bytes: &Bytes) -> anyhow::Result<Value> {
        let text = std::str::from_utf8(bytes)?;
        let data = text
            .lines()
            .find_map(|l| l.strip_prefix("data: "))
            .ok_or_else(|| anyhow::anyhow!("no data line in {text}"))?;
        Ok(serde_json::from_str(data)?)
    }

    #[test]
    fn the_second_reply_does_not_start_a_second_message() {
        let start = frame("message_start", r#"{"type":"message_start","message":{}}"#);
        assert!(onto_the_first_reply(start, 2).is_none());
    }

    #[test]
    fn its_blocks_are_numbered_after_the_blocks_the_client_already_has() -> anyhow::Result<()> {
        let delta = frame(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hi"}}"#,
        );
        let out = onto_the_first_reply(delta, 3)
            .ok_or_else(|| anyhow::anyhow!("a delta must be forwarded"))?;
        let json = data_of(&out)?;
        assert_eq!(json["index"], 4);
        assert_eq!(json["delta"]["text"], "hi");
        assert!(out.starts_with(b"event: content_block_delta\n"));
        Ok(())
    }

    #[test]
    fn with_nothing_sent_before_it_the_blocks_keep_their_bytes() {
        let raw = r#"{"type":"content_block_stop","index":0}"#;
        let stop = frame("content_block_stop", raw);
        let expected = stop.raw.clone();
        assert_eq!(onto_the_first_reply(stop, 0), Some(expected));
    }

    #[test]
    fn the_end_of_the_message_passes_through_unchanged() {
        let delta = frame(
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#,
        );
        let expected = delta.raw.clone();
        assert_eq!(onto_the_first_reply(delta, 5), Some(expected));
    }

    #[test]
    fn an_upstream_error_body_is_relayed_as_the_error_event_it_describes() -> anyhow::Result<()> {
        let body =
            br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let json = data_of(&upstream_error_frame(529, body))?;
        assert_eq!(json["error"]["type"], "overloaded_error");
        Ok(())
    }

    #[test]
    fn an_unreadable_upstream_error_names_the_status() -> anyhow::Result<()> {
        let json = data_of(&upstream_error_frame(502, b"<html>bad gateway</html>"))?;
        assert_eq!(json["type"], "error");
        let message = json["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("502"), "{message}");
        Ok(())
    }

    #[test]
    fn the_block_the_client_had_open_is_closed_at_its_own_index() -> anyhow::Result<()> {
        let json = data_of(&block_stop_frame(7))?;
        assert_eq!(json["type"], "content_block_stop");
        assert_eq!(json["index"], 7);
        Ok(())
    }
}
