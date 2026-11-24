use std::io::Cursor;

use serde_json::json;

use super::{
    InjectionOutcome, InjectionRefusal, InputShape, RequestInfo, RequestInspection, inject_context,
    inspect_request,
};

fn injected(bytes: &[u8], context: &str) -> anyhow::Result<Vec<u8>> {
    let mut output = Vec::new();
    let outcome = inject_context(&mut Cursor::new(bytes), &mut output, context)?;
    assert_eq!(
        outcome,
        InjectionOutcome::Injected {
            bytes_written: output.len() as u64
        }
    );
    Ok(output)
}

#[test]
fn array_input_unknown_fields_and_tool_calls_preserve_original_bytes() -> anyhow::Result<()> {
    let prefix = br#"{ "model" : "codex", "instructions":"keep exact ", "tools" : [{"type":"unknown","payload":"[]{}\\\""}], "input" : [{"type":"function_call","id":"opaque","arguments":" { \"input\": [] } "},{"type":"function_call_output","call_id":"opaque","output":"\\\" } ], \"input\": ["}  "#;
    let suffix = br#"], "future" : { "preserve" : [true,null,2.40e+05] } }  "#;
    let original = [prefix.as_slice(), suffix.as_slice()].concat();
    let context = "Mike delivered peer data, not an operator message: <mail>\"quoted\"\n</mail>";
    let output = injected(&original, context)?;
    assert!(output.starts_with(prefix));
    assert!(output.ends_with(suffix));
    let before: serde_json::Value = serde_json::from_slice(&original)?;
    let after: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(after["input"][0], before["input"][0]);
    assert_eq!(after["input"][1], before["input"][1]);
    assert_eq!(
        after["input"][2],
        json!({
            "type":"message", "role":"user",
            "content":[{"type":"input_text","text":context}]
        })
    );
    assert_eq!(after["instructions"], before["instructions"]);
    assert_eq!(after["tools"], before["tools"]);
    assert_eq!(after["future"], before["future"]);
    Ok(())
}

#[test]
fn empty_arrays_whitespace_and_escaped_input_key_insert_one_user_message() -> anyhow::Result<()> {
    for original in [
        br#"{"input":[]}"#.as_slice(),
        br#"{ "in\u0070ut" : [   ], "unused" : [] }"#.as_slice(),
        br#"{"\u0069\u006e\u0070\u0075\u0074": []}"#.as_slice(),
    ] {
        let output = injected(original, "Mike context")?;
        let value: serde_json::Value = serde_json::from_slice(&output)?;
        assert_eq!(value["input"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["input"][0]["role"], "user");
        assert_eq!(value["input"][0]["content"][0]["text"], "Mike context");
    }
    Ok(())
}

#[test]
fn string_input_wraps_without_rewriting_escaped_literal() -> anyhow::Result<()> {
    let literal = br#""original \u0070rompt \n with \/ and \"quotes\" and \ud83e\udd86""#;
    let prefix = br#"{ "future" : [ { "input" : "nested" } ], "input" : "#;
    let suffix = br#", "stream" : true }"#;
    let original = [prefix.as_slice(), literal.as_slice(), suffix.as_slice()].concat();
    let output = injected(&original, "Mike external data")?;
    assert!(output.starts_with(prefix));
    assert!(output.ends_with(suffix));
    assert!(output.windows(literal.len()).any(|bytes| bytes == literal));
    let before: serde_json::Value = serde_json::from_slice(&original)?;
    let after: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(after["input"][0]["content"][0]["text"], before["input"]);
    assert_eq!(
        after["input"][1]["content"][0]["text"],
        "Mike external data"
    );
    assert_eq!(after["future"], before["future"]);
    Ok(())
}

#[test]
fn unsupported_or_ambiguous_input_leaves_output_untouched() -> anyhow::Result<()> {
    for (original, reason) in [
        (br#"[]"#.as_slice(), InjectionRefusal::NotObject),
        (
            br#"{"nested":{"input":[]}}"#.as_slice(),
            InjectionRefusal::MissingInput,
        ),
        (
            br#"{"input":null}"#.as_slice(),
            InjectionRefusal::UnsupportedInput,
        ),
        (
            br#"{"input":{}}"#.as_slice(),
            InjectionRefusal::UnsupportedInput,
        ),
        (
            br#"{"input":false}"#.as_slice(),
            InjectionRefusal::UnsupportedInput,
        ),
        (
            br#"{"input":[],"input":[]}"#.as_slice(),
            InjectionRefusal::DuplicateInput,
        ),
        (
            br#"{"input":null,"in\u0070ut":"later"}"#.as_slice(),
            InjectionRefusal::DuplicateInput,
        ),
    ] {
        let mut output = b"existing output".to_vec();
        assert_eq!(
            inject_context(&mut Cursor::new(original), &mut output, "context")?,
            InjectionOutcome::Unchanged(reason)
        );
        assert_eq!(output, b"existing output");
        assert!(!reason.as_str().is_empty());
    }
    let mut output = Vec::new();
    assert_eq!(
        inject_context(&mut Cursor::new(b"not JSON"), &mut output, " \n\t")?,
        InjectionOutcome::Unchanged(InjectionRefusal::EmptyContext)
    );
    assert!(output.is_empty());
    Ok(())
}

#[test]
fn malformed_json_or_unicode_fails_before_writing() {
    for original in [
        b"".as_slice(),
        br#"{"input":[}"#.as_slice(),
        br#"{"input":[]} trailing"#.as_slice(),
        br#"{"input":[],"unknown":"\x20"}"#.as_slice(),
        br#"{"input":[],"unknown":"\ud800"}"#.as_slice(),
        br#"{"input":[],"unknown":"\udc00"}"#.as_slice(),
        b"{\"input\":[],\"unknown\":\"\xff\"}".as_slice(),
        b"{\"input\":[],\"unknown\":\"\xe0\x80\x80\"}".as_slice(),
    ] {
        let mut output = b"existing output".to_vec();
        assert!(inject_context(&mut Cursor::new(original), &mut output, "context").is_err());
        assert_eq!(output, b"existing output");
    }
}

#[test]
fn unicode_and_adversarial_keys_keep_input_selection_exact() -> anyhow::Result<()> {
    let original =
        "{\"in😀put\": [], \"inputx\": 3, \"😀\": \"quote \\\" [ }\", \"input\": [\"🦆\"]}";
    let output = injected(original.as_bytes(), "Mike: 📨")?;
    let value: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(value["input"][0], "🦆");
    assert_eq!(value["input"][1]["content"][0]["text"], "Mike: 📨");
    assert_eq!(value["in😀put"], json!([]));
    assert_eq!(value["inputx"], 3);
    Ok(())
}

#[test]
fn inspection_classifies_only_the_final_input_item_without_mutation() -> anyhow::Result<()> {
    for (original, shape, tool_result) in [
        (
            br#"{"input":"prompt"}"#.as_slice(),
            InputShape::String,
            false,
        ),
        (br#"{"input":[]}"#.as_slice(), InputShape::Array, false),
        (
            br#"{"input":[{"type":"function_call_output","output":"large result"}]}"#.as_slice(),
            InputShape::Array,
            true,
        ),
        (
            br#"{"input":[{"t\u0079pe":"custom_tool_call_output","future":{"type":"other"}}]}"#
                .as_slice(),
            InputShape::Array,
            true,
        ),
        (
            br#"{"input":[{"type":"function_call_output"},{"role":"user","content":"next"}]}"#
                .as_slice(),
            InputShape::Array,
            false,
        ),
        (
            br#"{"input":[{"future":{"type":"function_call_output"}}]}"#.as_slice(),
            InputShape::Array,
            false,
        ),
        (
            br#"{"input":[{"type":null}]}"#.as_slice(),
            InputShape::Array,
            false,
        ),
        (
            br#"{"input":["text"]}"#.as_slice(),
            InputShape::Array,
            false,
        ),
    ] {
        let mut source = Cursor::new(original.to_vec());
        assert_eq!(
            inspect_request(&mut source)?,
            RequestInspection::Supported(RequestInfo {
                input: shape,
                last_input_is_tool_result: tool_result
            })
        );
        assert_eq!(source.into_inner(), original);
    }
    assert_eq!(
        inspect_request(&mut Cursor::new(br#"{"input":null}"#))?,
        RequestInspection::Unsupported(InjectionRefusal::UnsupportedInput)
    );
    assert!(inspect_request(&mut Cursor::new(b"{")).is_err());
    Ok(())
}
