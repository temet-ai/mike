use serde::Serialize;
use serde_json::Value;

pub const DEFAULT_MAX_CHARS: usize = 500;
pub const MAX_CHARS_SETTING: &str = "lessons.shape.max_chars";

const STATUS_OPENERS: [&str; 11] = [
    "satisfied",
    "left unmet",
    "unmet",
    "done",
    "completed",
    "finished",
    "built",
    "implemented",
    "shipped",
    "merged",
    "committed",
];
const VERIFIER_TOPIC_PREFIX: &str = "verifier:";
const SHA_WINDOW_CHARS: usize = 80;
const SHA_MIN_LEN: usize = 7;
const SHA_MAX_LEN: usize = 40;
const UUID_LEN: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Rule,
    Status,
    OverLength,
}

impl Shape {
    pub fn code(self) -> &'static str {
        match self {
            Shape::Rule => "rule",
            Shape::Status => "status",
            Shape::OverLength => "over_length",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "rule" => Some(Shape::Rule),
            "status" => Some(Shape::Status),
            "over_length" => Some(Shape::OverLength),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    pub shape: Shape,
    pub detail: String,
    pub chars: usize,
}

impl Assessment {
    pub fn is_rule(&self) -> bool {
        self.shape == Shape::Rule
    }
}

pub fn max_chars_from_rows(rows: &[(String, Value)]) -> usize {
    rows.iter()
        .find(|(key, _)| key == MAX_CHARS_SETTING)
        .and_then(|(_, value)| value.as_u64())
        .and_then(|n| usize::try_from(n).ok())
        .map_or(DEFAULT_MAX_CHARS, |n| n.clamp(100, 5_000))
}

pub fn assess(text: &str, topic: Option<&str>, max_chars: usize) -> Assessment {
    let chars = text.chars().count();
    let (shape, detail) = if let Some(detail) = status_finding(text, topic) {
        (Shape::Status, detail)
    } else if chars > max_chars {
        (
            Shape::OverLength,
            format!("the text is {chars} characters and the limit is {max_chars}"),
        )
    } else {
        (Shape::Rule, "a rule".to_owned())
    };
    Assessment {
        shape,
        detail,
        chars,
    }
}

fn status_finding(text: &str, topic: Option<&str>) -> Option<String> {
    if let Some(topic) = topic.filter(|t| t.starts_with(VERIFIER_TOPIC_PREFIX)) {
        return Some(format!(
            "the topic {topic:?} marks a verifier carryover note, which reports on a session instead of stating a rule"
        ));
    }
    opening_status_word(text)
        .or_else(|| session_uuid_subject(text))
        .or_else(|| commit_sha_subject(text))
}

pub fn opens_with_status_word(text: &str) -> bool {
    opening_status_word(text).is_some()
}

fn opening_status_word(text: &str) -> Option<String> {
    let skipped = text.chars().take_while(|c| !c.is_alphanumeric()).count();
    let body: String = text.chars().skip(skipped).collect();
    let lower = body.to_lowercase();
    STATUS_OPENERS.iter().find_map(|opener| {
        let rest = lower.strip_prefix(opener)?;
        let boundary = rest.chars().next().is_none_or(|c| !c.is_alphanumeric());
        boundary.then(|| {
            let word: String = body.chars().take(opener.chars().count()).collect();
            format!(
                "the text opens with the status word {word:?} at character {skipped}, which reports an outcome instead of stating a rule"
            )
        })
    })
}

fn tokens_with_position(text: &str, keep: impl Fn(char) -> bool) -> Vec<(usize, String)> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut start = 0;
    for (index, c) in text.chars().enumerate() {
        if keep(c) {
            if current.is_empty() {
                start = index;
            }
            current.push(c);
        } else if !current.is_empty() {
            tokens.push((start, std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        tokens.push((start, current));
    }
    tokens
}

fn is_uuid(token: &str) -> bool {
    token.len() == UUID_LEN
        && token.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn session_uuid_subject(text: &str) -> Option<String> {
    tokens_with_position(text, |c| c.is_ascii_hexdigit() || c == '-')
        .into_iter()
        .find(|(_, token)| is_uuid(token))
        .map(|(position, token)| {
            format!(
                "the text names the session or object {token} at character {position}, an identifier that means nothing to a later session"
            )
        })
}

fn is_sha(token: &str) -> bool {
    (SHA_MIN_LEN..=SHA_MAX_LEN).contains(&token.len())
        && token.chars().all(|c| c.is_ascii_hexdigit())
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_alphabetic())
}

fn commit_sha_subject(text: &str) -> Option<String> {
    let window: String = text.chars().take(SHA_WINDOW_CHARS).collect();
    tokens_with_position(&window, |c| c.is_ascii_alphanumeric())
        .into_iter()
        .find(|(_, token)| is_sha(token))
        .map(|(position, token)| {
            format!(
                "the text names the commit {token} at character {position} as its subject, which is history instead of a rule"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape_of(text: &str, topic: Option<&str>) -> Shape {
        assess(text, topic, DEFAULT_MAX_CHARS).shape
    }

    #[test]
    fn imperative_rule_is_a_rule() {
        let text = "Run scripts/gate lint before committing; a green cargo check alone does not prove the structural limits hold.";
        assert_eq!(shape_of(text, None), Shape::Rule);
    }

    #[test]
    fn status_opening_words_are_flagged_with_their_position() {
        let found = assess(
            "Satisfied. The ended session's style-check work was integrated.",
            None,
            DEFAULT_MAX_CHARS,
        );
        assert_eq!(found.shape, Shape::Status);
        assert!(found.detail.contains("\"Satisfied\""), "{}", found.detail);
        assert!(found.detail.contains("character 0"), "{}", found.detail);
    }

    #[test]
    fn left_unmet_and_built_open_as_status() {
        assert_eq!(
            shape_of("Left unmet when session ended: nothing was verified", None),
            Shape::Status
        );
        assert_eq!(
            shape_of("Built the producer as a Stop hook", None),
            Shape::Status
        );
    }

    #[test]
    fn a_status_word_inside_a_longer_word_is_not_an_opener() {
        assert_eq!(
            shape_of(
                "Doneness checks must run the command, not read the diff.",
                None
            ),
            Shape::Rule
        );
    }

    #[test]
    fn leading_markdown_does_not_hide_the_opener() {
        assert_eq!(shape_of("**Done** - the gate passed", None), Shape::Status);
    }

    #[test]
    fn verifier_topic_marks_a_status_note_whatever_the_text_says() {
        let found = assess(
            "Check the dispatch record before relying on it.",
            Some("verifier:unmet"),
            DEFAULT_MAX_CHARS,
        );
        assert_eq!(found.shape, Shape::Status);
        assert!(found.detail.contains("verifier:unmet"), "{}", found.detail);
    }

    #[test]
    fn session_uuid_anywhere_is_flagged() {
        let found = assess(
            "Stop hooks fail silently when the session 1bd1159e-57bb-46ee-955b-2ea18fa6d5e2 is resumed.",
            None,
            DEFAULT_MAX_CHARS,
        );
        assert_eq!(found.shape, Shape::Status);
        assert!(
            found
                .detail
                .contains("1bd1159e-57bb-46ee-955b-2ea18fa6d5e2"),
            "{}",
            found.detail
        );
    }

    #[test]
    fn commit_sha_early_in_the_text_is_flagged_but_late_is_not() {
        assert_eq!(
            shape_of(
                "Commit 2dd7b8d6 split FilterRule::validate into two functions",
                None
            ),
            Shape::Status
        );
        let late = format!(
            "{} see 2dd7b8d6",
            "Keep functions under one hundred lines. ".repeat(3)
        );
        assert_eq!(shape_of(&late, None), Shape::Rule);
    }

    #[test]
    fn plain_words_made_of_hex_letters_are_not_shas() {
        assert_eq!(
            shape_of(
                "Always decade facade checks before claiming a build is clean",
                None
            ),
            Shape::Rule
        );
    }

    #[test]
    fn long_text_is_over_length_and_names_the_limit() {
        let text = "Verify every claim. ".repeat(40);
        let found = assess(&text, None, 500);
        assert_eq!(found.shape, Shape::OverLength);
        assert_eq!(found.chars, text.chars().count());
        assert!(found.detail.contains("500"), "{}", found.detail);
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        let text = "é".repeat(300);
        assert_eq!(shape_of(&text, None), Shape::Rule);
    }

    #[test]
    fn max_chars_setting_is_read_and_clamped() {
        assert_eq!(max_chars_from_rows(&[]), DEFAULT_MAX_CHARS);
        let rows = vec![(MAX_CHARS_SETTING.to_owned(), json!(800))];
        assert_eq!(max_chars_from_rows(&rows), 800);
        let low = vec![(MAX_CHARS_SETTING.to_owned(), json!(5))];
        assert_eq!(max_chars_from_rows(&low), 100);
        let wrong_type = vec![(MAX_CHARS_SETTING.to_owned(), json!("many"))];
        assert_eq!(max_chars_from_rows(&wrong_type), DEFAULT_MAX_CHARS);
    }

    #[test]
    fn shape_codes_round_trip() {
        for shape in [Shape::Rule, Shape::Status, Shape::OverLength] {
            assert_eq!(Shape::from_code(shape.code()), Some(shape));
        }
        assert_eq!(Shape::from_code("other"), None);
    }
}
