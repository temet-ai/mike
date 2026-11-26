pub const MARKER: &str = "mike-keep:";
pub const MIN_WORDS: usize = 5;
const CLOSERS: [&str; 3] = ["*/", "-->", "#}"];

pub fn reason_beside(lines: &[&str], line: u32, rule_id: &str) -> Option<String> {
    let at = usize::try_from(line).ok()?.checked_sub(1)?;
    let above = at.checked_sub(1).and_then(|i| lines.get(i));
    [lines.get(at), above]
        .into_iter()
        .flatten()
        .find_map(|text| reason_in(text, rule_id))
}

fn reason_in(text: &str, rule_id: &str) -> Option<String> {
    let (_, after) = text.split_once(MARKER)?;
    let (named, reason) = after.trim_start().split_once(char::is_whitespace)?;
    if named.trim_end_matches([',', ':']) != rule_id {
        return None;
    }
    let mut reason = reason.trim();
    for closer in CLOSERS {
        reason = reason.trim_end_matches(closer).trim_end();
    }
    (reason.split_whitespace().count() >= MIN_WORDS).then(|| reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "ts-html-sink";

    #[test]
    fn a_reason_on_the_line_or_the_line_above_is_read() {
        let same = [
            "el.innerHTML = page; // mike-keep: ts-html-sink page is built here from escaped parts",
        ];
        assert_eq!(
            reason_beside(&same, 1, RULE).as_deref(),
            Some("page is built here from escaped parts")
        );
        let above = [
            "/* mike-keep: ts-html-sink the frame is ours and holds no outside text */",
            "f.contentDocument.body.innerHTML = html;",
        ];
        assert_eq!(
            reason_beside(&above, 2, RULE).as_deref(),
            Some("the frame is ours and holds no outside text")
        );
    }

    #[test]
    fn a_marker_without_a_reason_or_for_another_rule_keeps_nothing() {
        let bare = ["el.innerHTML = x; // mike-keep: ts-html-sink"];
        assert_eq!(reason_beside(&bare, 1, RULE), None);
        let short = ["el.innerHTML = x; // mike-keep: ts-html-sink it is fine"];
        assert_eq!(reason_beside(&short, 1, RULE), None);
        let other =
            ["el.innerHTML = x; // mike-keep: ts-eval this one is about something else entirely"];
        assert_eq!(reason_beside(&other, 1, RULE), None);
        let far = [
            "// mike-keep: ts-html-sink this marker sits two lines up from the code",
            "",
            "el.innerHTML = x;",
        ];
        assert_eq!(reason_beside(&far, 3, RULE), None);
    }
}
