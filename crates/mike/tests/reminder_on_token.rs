use mike::reminder_on_token::{Intent, Outcome, apply, parse_token};
use serde_json::{Value, json};

const PUA_START: char = '\u{E000}';
const PUA_END: char = '\u{E001}';

const HOOK_R1_TOKEN_B64: &str = "eyJ2IjogMSwgImludGVudCI6ICJyMSIsICJwYXR0ZXJuIjogIlR1cm5PcmlnaW58T3BlcmF0b3JUdXJufFNlc3Npb25IZWFkIiwgInBhdGhzIjogWyJjcmF0ZXMvIl0sICJmbGFncyI6IFsiLXJsIl19";
const HOOK_R2_TOKEN_B64: &str = "eyJ2IjogMSwgImludGVudCI6ICJyMiIsICJwYXR0ZXJuIjogIlR1cm5PcmlnaW4iLCAicGF0aHMiOiBbImNyYXRlcy8iXSwgImZsYWdzIjogWyItcm4iLCAiLS1pbmNsdWRlPSoucnMiXX0=";

fn body_with_r2_token(scope_paths: &[&str]) -> anyhow::Result<Vec<u8>> {
    let payload = json!({
        "v": 1,
        "intent": "r2",
        "pattern": "NeedleXYZ",
        "paths": scope_paths,
        "flags": ["-rn"],
    });
    let b64 = base64url_encode(serde_json::to_vec(&payload)?.as_slice());
    body_with_raw_token(&format!("{PUA_START}{b64}{PUA_END}"))
}

fn body_with_raw_token(raw_token: &str) -> anyhow::Result<Vec<u8>> {
    let text = format!(
        "<system-reminder>\nPostToolUse:Bash hook additional context: {raw_token}\n</system-reminder>"
    );
    Ok(serde_json::to_vec(&json!({
        "model": "claude-sonnet-4",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "run the grep"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "id1", "content": "grep output"},
                {"type": "text", "text": text,
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

fn has_pua(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|w| w == [0xEE, 0x80, 0x80])
}

#[tokio::test]
async fn no_token_body_passes_through_byte_identical() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}],
    }))?;
    assert_eq!(apply(&body, true).await, Outcome::Unchanged);
    Ok(())
}

#[tokio::test]
async fn r2_hit_replaces_hook_block_and_strips_token() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("hit.rs"),
        "let x = NeedleXYZ_marker();\nlet y = NeedleXYZ_marker();\n",
    )?;
    let scope = dir.path().to_string_lossy().into_owned();

    let body = body_with_r2_token(&[&scope])?;
    let out = apply(&body, true).await;

    let Outcome::Resolved { new_body } = out else {
        panic!("expected Resolved for an R2 hit, got {out:?}");
    };
    assert!(
        !has_pua(&new_body),
        "raw token must be absent from forwarded body"
    );
    let content = last_user_content(&new_body)?;
    assert_eq!(content.len(), 2, "tool_result + replaced text block");
    assert_eq!(content[0]["type"], "tool_result");
    let text = content[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("content[1].text is not a string"))?;
    assert_eq!(
        text.matches("<system-reminder>").count(),
        1,
        "exactly one <system-reminder> shell, no double-wrap"
    );
    assert!(
        text.contains("literal where-used"),
        "resolved literal answer must be spliced in: {text}"
    );
    assert_eq!(
        content[1]["cache_control"]["ttl"], "1h",
        "the hook block's cache_control must be preserved"
    );
    Ok(())
}

#[tokio::test]
async fn toggle_off_strips_and_drops_empty_shell() -> anyhow::Result<()> {
    let body = body_with_r2_token(&["/nonexistent-scope-xyz"])?;
    let out = apply(&body, false).await;

    let Outcome::Stripped { new_body } = out else {
        panic!("expected Stripped when toggle off, got {out:?}");
    };
    assert!(!has_pua(&new_body));
    let content = last_user_content(&new_body)?;
    assert_eq!(
        content.len(),
        1,
        "empty hook shell dropped, tool_result remains"
    );
    assert!(
        !String::from_utf8_lossy(&new_body).contains("<system-reminder>"),
        "no residual empty <system-reminder> shell may be forwarded"
    );
    Ok(())
}

#[tokio::test]
async fn r2_resolver_error_strips_only() -> anyhow::Result<()> {
    let body = body_with_r2_token(&["/definitely/not/a/real/path/zzz"])?;
    let out = apply(&body, true).await;
    assert!(
        matches!(out, Outcome::Stripped { .. }),
        "resolver error must fail open to strip-only, got {out:?}"
    );
    let Outcome::Stripped { new_body } = out else {
        unreachable!()
    };
    assert!(!has_pua(&new_body));
    Ok(())
}

#[tokio::test]
async fn r2_resolver_miss_strips_only() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("nothing.rs"), "let z = 1;\n")?;
    let scope = dir.path().to_string_lossy().into_owned();

    let body = body_with_r2_token(&[&scope])?;
    let out = apply(&body, true).await;
    assert!(
        matches!(out, Outcome::Stripped { .. }),
        "0-hit resolver result must strip-only, got {out:?}"
    );
    Ok(())
}

#[tokio::test]
async fn malformed_payload_strips_only() -> anyhow::Result<()> {
    let body = body_with_raw_token(&format!("{PUA_START}!!!not-base64-json!!!{PUA_END}"))?;
    let out = apply(&body, true).await;
    assert!(
        matches!(out, Outcome::Stripped { .. }),
        "malformed payload must strip-only (never 4xx/5xx), got {out:?}"
    );
    let Outcome::Stripped { new_body } = out else {
        unreachable!()
    };
    assert!(!has_pua(&new_body));
    Ok(())
}

#[tokio::test]
async fn captured_body_retains_raw_token_forwarded_does_not() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("hit.rs"), "NeedleXYZ_marker\n")?;
    let scope = dir.path().to_string_lossy().into_owned();

    let body = body_with_r2_token(&[&scope])?;
    let captured = body.clone();

    let out = apply(&body, true).await;
    let new_body = match out {
        Outcome::Resolved { new_body } | Outcome::Stripped { new_body } => new_body,
        Outcome::Unchanged => panic!("token present, expected a mutation"),
    };

    assert_eq!(body, captured, "apply must not mutate its input in place");
    assert!(
        has_pua(&captured),
        "captured body retains the raw token (fidelity)"
    );
    assert!(
        !has_pua(&new_body),
        "forwarded body has the raw token stripped"
    );
    Ok(())
}

#[test]
fn rust_parser_accepts_hook_emitted_r2_token() -> anyhow::Result<()> {
    let block = format!(
        "<system-reminder>\nPostToolUse:Bash hook additional context: {PUA_START}{HOOK_R2_TOKEN_B64}{PUA_END}\n</system-reminder>"
    );
    let payload = parse_token(&block)
        .ok_or_else(|| anyhow::anyhow!("Rust parser must accept the hook's r2 token"))?;
    assert_eq!(payload.version, 1);
    assert_eq!(payload.intent, Intent::R2);
    assert_eq!(payload.pattern, "TurnOrigin");
    assert_eq!(payload.paths, vec!["crates/"]);
    assert_eq!(payload.flags, vec!["-rn", "--include=*.rs"]);
    Ok(())
}

#[test]
fn rust_parser_accepts_hook_emitted_r1_token() -> anyhow::Result<()> {
    let block = format!("{PUA_START}{HOOK_R1_TOKEN_B64}{PUA_END}");
    let payload = parse_token(&block)
        .ok_or_else(|| anyhow::anyhow!("Rust parser must accept the hook's r1 token"))?;
    assert_eq!(payload.intent, Intent::R1);
    assert_eq!(payload.pattern, "TurnOrigin|OperatorTurn|SessionHead");
    assert_eq!(payload.paths, vec!["crates/"]);
    assert_eq!(payload.flags, vec!["-rl"]);
    Ok(())
}

fn base64url_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
