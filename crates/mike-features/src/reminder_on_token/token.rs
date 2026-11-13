use serde::Deserialize;

pub const PUA_START: char = '\u{E000}';
pub const PUA_END: char = '\u{E001}';
pub const PUA_START_BYTES: &[u8] = &[0xEE, 0x80, 0x80];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    R1,
    R2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub version: u8,
    pub intent: Intent,
    pub pattern: String,
    pub paths: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Deserialize)]
struct RawPayload {
    v: u8,
    intent: String,
    pattern: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    flags: Vec<String>,
}

pub struct TokenSpan {
    pub start: usize,
    pub end: usize,
    pub inner_b64: Option<String>,
}

pub fn find_span(text: &str) -> Option<TokenSpan> {
    let start = text.find(PUA_START)?;
    let after = start + PUA_START.len_utf8();
    match text[after..].find(PUA_END) {
        Some(rel) => {
            let end_delim = after + rel;
            Some(TokenSpan {
                start,
                end: end_delim + PUA_END.len_utf8(),
                inner_b64: Some(text[after..end_delim].to_string()),
            })
        }
        None => Some(TokenSpan {
            start,
            end: text.len(),
            inner_b64: None,
        }),
    }
}

pub fn parse_payload(b64: &str) -> Option<Payload> {
    let json = base64url_decode(b64)?;
    let raw: RawPayload = serde_json::from_slice(&json).ok()?;
    if raw.v != 1 {
        return None;
    }
    let intent = match raw.intent.to_ascii_lowercase().as_str() {
        "r1" => Intent::R1,
        "r2" => Intent::R2,
        _ => return None,
    };
    Some(Payload {
        version: raw.v,
        intent,
        pattern: raw.pattern,
        paths: raw.paths,
        flags: raw.flags,
    })
}

pub fn parse_token(text: &str) -> Option<Payload> {
    let span = find_span(text)?;
    parse_payload(span.inner_b64.as_deref()?)
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut nbits: u32 = 0;
    for &b in input.as_bytes() {
        if b == b'=' {
            continue;
        }
        acc = (acc << 6) | sextet(b)? as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOK_R1_B64: &str = "eyJ2IjogMSwgImludGVudCI6ICJyMSIsICJwYXR0ZXJuIjogIlR1cm5PcmlnaW58T3BlcmF0b3JUdXJufFNlc3Npb25IZWFkIiwgInBhdGhzIjogWyJjcmF0ZXMvIl0sICJmbGFncyI6IFsiLXJsIl19";
    const HOOK_R2_B64: &str = "eyJ2IjogMSwgImludGVudCI6ICJyMiIsICJwYXR0ZXJuIjogIlR1cm5PcmlnaW4iLCAicGF0aHMiOiBbImNyYXRlcy8iXSwgImZsYWdzIjogWyItcm4iLCAiLS1pbmNsdWRlPSoucnMiXX0=";

    #[test]
    fn parses_hook_r1_token_verbatim() -> anyhow::Result<()> {
        let p = parse_payload(HOOK_R1_B64).ok_or_else(|| anyhow::anyhow!("r1 payload decodes"))?;
        assert_eq!(p.version, 1);
        assert_eq!(p.intent, Intent::R1);
        assert_eq!(p.pattern, "TurnOrigin|OperatorTurn|SessionHead");
        assert_eq!(p.paths, vec!["crates/"]);
        assert_eq!(p.flags, vec!["-rl"]);
        Ok(())
    }

    #[test]
    fn parses_hook_r2_token_with_padding() -> anyhow::Result<()> {
        let p = parse_payload(HOOK_R2_B64).ok_or_else(|| anyhow::anyhow!("r2 payload decodes"))?;
        assert_eq!(p.intent, Intent::R2);
        assert_eq!(p.pattern, "TurnOrigin");
        assert_eq!(p.paths, vec!["crates/"]);
        assert_eq!(p.flags, vec!["-rn", "--include=*.rs"]);
        Ok(())
    }

    #[test]
    fn find_span_locates_mid_string_token() -> anyhow::Result<()> {
        let text = format!(
            "<system-reminder>\nPostToolUse:Bash hook additional context: {PUA_START}{HOOK_R2_B64}{PUA_END}\n</system-reminder>"
        );
        let span = find_span(&text).ok_or_else(|| anyhow::anyhow!("span found"))?;
        assert!(span.start > 0, "token is mid-string, not at offset 0");
        assert_eq!(span.inner_b64.as_deref(), Some(HOOK_R2_B64));
        assert_eq!(
            &text[span.start..span.end],
            format!("{PUA_START}{HOOK_R2_B64}{PUA_END}")
        );
        Ok(())
    }

    #[test]
    fn find_span_unterminated_covers_to_end() -> anyhow::Result<()> {
        let text = format!("prefix {PUA_START}garbage-no-end");
        let span = find_span(&text).ok_or_else(|| anyhow::anyhow!("start delimiter found"))?;
        assert_eq!(span.end, text.len());
        assert!(span.inner_b64.is_none());
        Ok(())
    }

    #[test]
    fn parse_token_extracts_from_full_block() -> anyhow::Result<()> {
        let text = format!("noise {PUA_START}{HOOK_R1_B64}{PUA_END} trailing");
        let p = parse_token(&text)
            .ok_or_else(|| anyhow::anyhow!("token parses from full block text"))?;
        assert_eq!(p.intent, Intent::R1);
        Ok(())
    }

    #[test]
    fn parse_payload_rejects_bad_base64() {
        assert!(parse_payload("not valid base64 !!!").is_none());
    }

    #[test]
    fn parse_payload_rejects_wrong_version() {
        let b64 = base64url_encode(br#"{"v":2,"intent":"r2","pattern":"x"}"#);
        assert!(parse_payload(&b64).is_none());
    }

    #[test]
    fn parse_payload_rejects_unknown_intent() {
        let b64 = base64url_encode(br#"{"v":1,"intent":"r9","pattern":"x"}"#);
        assert!(parse_payload(&b64).is_none());
    }

    #[test]
    fn parse_payload_rejects_non_json() {
        let b64 = base64url_encode(b"this is not json");
        assert!(parse_payload(&b64).is_none());
    }

    #[test]
    fn base64url_roundtrip_no_padding_and_padded() {
        for raw in [&b"abc"[..], b"abcd", b"abcde", b"hello world"] {
            let enc = base64url_encode(raw);
            assert_eq!(base64url_decode(&enc).as_deref(), Some(raw));
        }
    }

    fn base64url_encode(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
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
}
