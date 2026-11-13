pub mod scan;
pub mod stage;
pub mod token;

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

pub use token::{Intent, Payload, parse_token};

use token::{PUA_START_BYTES, TokenSpan};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Resolved { new_body: Vec<u8> },
    Stripped { new_body: Vec<u8> },
}

pub async fn apply(body: &[u8], enabled: bool) -> Outcome {
    apply_with_resolver(body, enabled, DEFAULT_TIMEOUT, real_resolve).await
}

pub async fn apply_with_resolver<F>(
    body: &[u8],
    enabled: bool,
    timeout: Duration,
    resolve: F,
) -> Outcome
where
    F: FnOnce(Payload) -> Option<String> + Send + 'static,
{
    if !contains_marker(body) {
        return Outcome::Unchanged;
    }
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return Outcome::Unchanged;
    };
    let Some((msg_idx, block_idx)) = scan::locate(&value) else {
        return Outcome::Unchanged;
    };
    let text = block_text(&value, msg_idx, block_idx).unwrap_or_default();
    let Some(span) = token::find_span(&text) else {
        return Outcome::Unchanged;
    };

    let payload = span.inner_b64.as_deref().and_then(token::parse_payload);

    let answer = match payload {
        Some(p) if enabled => run_resolver(resolve, p, timeout).await,
        _ => None,
    };

    match answer {
        Some(ans) => {
            replace_block(&mut value, msg_idx, block_idx, &ans);
            serialize(value, true)
        }
        None => {
            strip_block(&mut value, msg_idx, block_idx, &text, &span);
            serialize(value, false)
        }
    }
}

fn serialize(value: Value, resolved: bool) -> Outcome {
    match serde_json::to_vec(&value) {
        Ok(bytes) if resolved => Outcome::Resolved { new_body: bytes },
        Ok(bytes) => Outcome::Stripped { new_body: bytes },
        Err(_) => Outcome::Unchanged,
    }
}

fn contains_marker(body: &[u8]) -> bool {
    body.windows(PUA_START_BYTES.len())
        .any(|w| w == PUA_START_BYTES)
}

fn block_text(value: &Value, msg_idx: usize, block_idx: usize) -> Option<String> {
    value
        .get("messages")?
        .get(msg_idx)?
        .get("content")?
        .get(block_idx)?
        .get("text")?
        .as_str()
        .map(str::to_owned)
}

fn block_mut(value: &mut Value, msg_idx: usize, block_idx: usize) -> Option<&mut Value> {
    value
        .get_mut("messages")?
        .get_mut(msg_idx)?
        .get_mut("content")?
        .get_mut(block_idx)
}

fn replace_block(value: &mut Value, msg_idx: usize, block_idx: usize, answer: &str) {
    if let Some(block) = block_mut(value, msg_idx, block_idx).and_then(Value::as_object_mut) {
        block.insert(
            "text".to_string(),
            Value::String(format!("<system-reminder>\n{answer}\n</system-reminder>")),
        );
    }
}

fn strip_block(value: &mut Value, msg_idx: usize, block_idx: usize, text: &str, span: &TokenSpan) {
    let stripped = format!("{}{}", &text[..span.start], &text[span.end..]);
    if is_empty_hook_shell(&stripped) {
        if let Some(content) = value
            .get_mut("messages")
            .and_then(|m| m.get_mut(msg_idx))
            .and_then(|mm| mm.get_mut("content"))
            .and_then(Value::as_array_mut)
            && block_idx < content.len()
        {
            content.remove(block_idx);
        }
    } else if let Some(block) = block_mut(value, msg_idx, block_idx).and_then(Value::as_object_mut)
    {
        block.insert("text".to_string(), Value::String(stripped));
    }
}

fn is_empty_hook_shell(text: &str) -> bool {
    let mut inner = text.trim();
    inner = inner
        .strip_prefix("<system-reminder>")
        .unwrap_or(inner)
        .trim();
    inner = inner
        .strip_suffix("</system-reminder>")
        .unwrap_or(inner)
        .trim();
    if let Some(pos) = inner.find("hook additional context:") {
        inner = inner[pos + "hook additional context:".len()..].trim();
    }
    inner.is_empty()
}

async fn run_resolver<F>(resolve: F, payload: Payload, timeout: Duration) -> Option<String>
where
    F: FnOnce(Payload) -> Option<String> + Send + 'static,
{
    match tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || resolve(payload)),
    )
    .await
    {
        Ok(Ok(answer)) => answer,
        Ok(Err(_join_err)) => None,
        Err(_elapsed) => None,
    }
}

fn real_resolve(payload: Payload) -> Option<String> {
    let scope = payload
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    match payload.intent {
        Intent::R2 => {
            let query = crate::resolvers::literal::LiteralQuery::new(payload.pattern, &scope);
            let answer = crate::resolvers::literal::resolve(&query).ok()?;
            (answer.total_hits > 0).then(|| answer.render())
        }
        Intent::R1 => {
            let keywords: Vec<String> = payload
                .pattern
                .split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let keywords = if keywords.is_empty() {
                vec![payload.pattern]
            } else {
                keywords
            };
            let manifest =
                crate::resolvers::manifest::build_manifest(&keywords, Path::new(&scope)).ok()?;
            let empty = manifest.entries.is_empty() && manifest.file_list.is_empty();
            (!empty).then(|| manifest.render())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const B64: &str = "eyJ2IjogMSwgImludGVudCI6ICJyMiIsICJwYXR0ZXJuIjogIkZvb0JhciIsICJwYXRocyI6IFtdLCAiZmxhZ3MiOiBbXX0=";

    fn hook_block_text(inner_token: &str) -> String {
        format!(
            "<system-reminder>\nPostToolUse:Bash hook additional context: {}{inner_token}{}\n</system-reminder>",
            '\u{E000}', '\u{E001}'
        )
    }

    fn body_with_token(inner_token: &str) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(&json!({
            "model": "claude-sonnet-4",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "go"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "id1", "content": "grep output"},
                    {"type": "text", "text": hook_block_text(inner_token),
                     "cache_control": {"type": "ephemeral", "ttl": "1h"}},
                ]},
            ],
        }))?)
    }

    fn last_user_content(body: &[u8]) -> anyhow::Result<Vec<Value>> {
        let v: Value = serde_json::from_slice(body)?;
        v["messages"][2]["content"]
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("messages[2].content is not an array"))
    }

    #[tokio::test]
    async fn no_token_passes_through_byte_identical() -> anyhow::Result<()> {
        let body = serde_json::to_vec(&json!({
            "messages": [{"role": "user", "content": [{"type": "text", "text": "plain"}]}],
        }))?;
        assert_eq!(
            apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| Some("x".into())).await,
            Outcome::Unchanged
        );
        Ok(())
    }

    #[tokio::test]
    async fn resolver_hit_replaces_block_in_place() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let out = apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| {
            Some("RESOLVED ANSWER".to_string())
        })
        .await;
        let Outcome::Resolved { new_body } = out else {
            panic!("expected Resolved, got {out:?}");
        };
        assert!(!contains_marker(&new_body), "raw token must be stripped");
        let content = last_user_content(&new_body)?;
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "tool_result");
        let text = content[1]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("content[1].text is not a string"))?;
        assert_eq!(
            text,
            "<system-reminder>\nRESOLVED ANSWER\n</system-reminder>"
        );
        assert_eq!(text.matches("<system-reminder>").count(), 1);
        assert_eq!(content[1]["cache_control"]["ttl"], "1h");
        Ok(())
    }

    #[tokio::test]
    async fn toggle_off_strips_without_injecting_or_leaving_empty_shell() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let out = apply_with_resolver(&body, false, DEFAULT_TIMEOUT, |_| {
            panic!("resolver must not run when disabled")
        })
        .await;
        let Outcome::Stripped { new_body } = out else {
            panic!("expected Stripped, got {out:?}");
        };
        assert!(!contains_marker(&new_body));
        let content = last_user_content(&new_body)?;
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_result");
        assert!(!String::from_utf8_lossy(&new_body).contains("<system-reminder>"));
        Ok(())
    }

    #[tokio::test]
    async fn resolver_miss_strips_only() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let out = apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| None).await;
        let Outcome::Stripped { new_body } = out else {
            panic!("expected Stripped on miss, got {out:?}");
        };
        assert!(!contains_marker(&new_body));
        assert_eq!(last_user_content(&new_body)?.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn resolver_timeout_strips_only() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let out = apply_with_resolver(&body, true, Duration::from_millis(30), |_| {
            std::thread::sleep(Duration::from_millis(100));
            Some("too late".to_string())
        })
        .await;
        assert!(
            matches!(out, Outcome::Stripped { .. }),
            "timeout must fail open to strip-only, got {out:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn resolver_panic_strips_only() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let out = apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| {
            panic!("resolver blew up");
        })
        .await;
        assert!(
            matches!(out, Outcome::Stripped { .. }),
            "resolver panic must fail open to strip-only, got {out:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn malformed_payload_strips_only_never_errors() -> anyhow::Result<()> {
        let body = body_with_token("!!!not-base64!!!")?;
        let out = apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| {
            panic!("resolver must not run for a malformed payload")
        })
        .await;
        assert!(matches!(out, Outcome::Stripped { .. }));
        let Outcome::Stripped { new_body } = out else {
            unreachable!()
        };
        assert!(!contains_marker(&new_body));
        Ok(())
    }

    #[tokio::test]
    async fn input_bytes_never_mutated_in_place() -> anyhow::Result<()> {
        let body = body_with_token(B64)?;
        let original = body.clone();
        let _ = apply_with_resolver(&body, true, DEFAULT_TIMEOUT, |_| Some("x".into())).await;
        assert_eq!(body, original, "input buffer must not be mutated");
        assert!(contains_marker(&body), "input still carries the raw token");
        Ok(())
    }

    #[test]
    fn is_empty_hook_shell_detects_payloadless_shell() {
        assert!(is_empty_hook_shell(
            "<system-reminder>\nPostToolUse:Bash hook additional context: \n</system-reminder>"
        ));
        assert!(!is_empty_hook_shell(
            "<system-reminder>\nPostToolUse:Bash hook additional context: real content\n</system-reminder>"
        ));
    }

    #[test]
    fn non_hook_block_with_leftover_text_is_kept() {
        assert!(!is_empty_hook_shell(
            "<system-reminder>\nkeep me\n</system-reminder>"
        ));
    }
}
