use serde_json::Value;

use super::detect::{REDUCTION_MARKER_TAG, extract_content};

pub struct OmittedRange {
    file: String,
    start: u64,
    end: u64,
}

pub fn read_offset(input: &Value) -> u64 {
    input
        .get("offset")
        .and_then(Value::as_u64)
        .filter(|offset| *offset >= 1)
        .unwrap_or(1)
}

fn parse_marker(after_tag: &str) -> Option<OmittedRange> {
    let after_file = after_tag.split_once(" file=")?.1;
    let (file, after_lines) = after_file.split_once(" result_lines=")?;
    let omitted = after_lines.split_once(" omitted=")?.1;
    let (start, remainder) = omitted.split_once('-')?;
    let end: String = remainder.chars().take_while(char::is_ascii_digit).collect();
    Some(OmittedRange {
        file: file.to_string(),
        start: start.parse().ok()?,
        end: end.parse().ok()?,
    })
}

pub fn collect_omitted_ranges(messages: &[Value]) -> Vec<OmittedRange> {
    let mut ranges = Vec::new();
    for message in messages {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some((text, _)) = extract_content(block, "") else {
                continue;
            };
            ranges.extend(
                text.match_indices(REDUCTION_MARKER_TAG)
                    .filter_map(|(at, _)| parse_marker(&text[at + REDUCTION_MARKER_TAG.len()..])),
            );
        }
    }
    ranges
}

pub fn is_recovery_read(input: &Value, file_path: &str, ranges: &[OmittedRange]) -> bool {
    let offset = read_offset(input);
    ranges
        .iter()
        .any(|range| range.file == file_path && range.start <= offset && offset <= range.end)
}
