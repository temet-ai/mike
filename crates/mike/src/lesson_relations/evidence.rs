use chrono::{DateTime, Utc};
use serde::Serialize;

const POLARITY_MARKERS: &[&str] = &[
    "RESOLVED",
    "FIXED",
    "corrected",
    "superseded",
    "was incomplete",
    "no longer",
    "root cause",
];

const SPAN_CONTEXT_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct QuotedSpan {
    pub(crate) marker: &'static str,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) quote: String,
    pub(crate) context_start: usize,
    pub(crate) context_end: usize,
    pub(crate) context: String,
}

pub(crate) fn extract_polarity_spans(text: &str) -> Vec<QuotedSpan> {
    let haystack = text.to_ascii_lowercase();
    let total_chars = text.chars().count();
    let mut spans = Vec::new();

    for marker in POLARITY_MARKERS {
        let needle = marker.to_ascii_lowercase();
        let mut search_from = 0usize;
        while let Some(rel_pos) = haystack[search_from..].find(&needle) {
            let byte_pos = search_from + rel_pos;
            let char_pos = text[..byte_pos].chars().count();
            let marker_chars = marker.chars().count();

            let start = char_pos;
            let end = char_pos + marker_chars;
            let quote: String = text.chars().skip(start).take(marker_chars).collect();

            let context_start = char_pos.saturating_sub(SPAN_CONTEXT_CHARS);
            let context_end = (end + SPAN_CONTEXT_CHARS).min(total_chars);
            let context: String = text
                .chars()
                .skip(context_start)
                .take(context_end - context_start)
                .collect();

            spans.push(QuotedSpan {
                marker,
                start,
                end,
                quote,
                context_start,
                context_end,
                context,
            });
            search_from = byte_pos + needle.len().max(1);
        }
    }
    spans
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvidenceBundle {
    pub(crate) lesson_a_id: i64,
    pub(crate) lesson_a_text: String,
    pub(crate) lesson_a_created_at: DateTime<Utc>,
    pub(crate) lesson_a_spans: Vec<QuotedSpan>,
    pub(crate) lesson_b_id: i64,
    pub(crate) lesson_b_text: String,
    pub(crate) lesson_b_created_at: DateTime<Utc>,
    pub(crate) lesson_b_spans: Vec<QuotedSpan>,
    pub(crate) cosine_distance: f64,
}

pub(crate) struct EvidenceBundleParams {
    pub(crate) lesson_a_id: i64,
    pub(crate) lesson_a_text: String,
    pub(crate) lesson_a_created_at: DateTime<Utc>,
    pub(crate) lesson_b_id: i64,
    pub(crate) lesson_b_text: String,
    pub(crate) lesson_b_created_at: DateTime<Utc>,
    pub(crate) cosine_distance: f64,
}

impl EvidenceBundle {
    pub(crate) fn build(params: EvidenceBundleParams) -> Self {
        let lesson_a_spans = extract_polarity_spans(&params.lesson_a_text);
        let lesson_b_spans = extract_polarity_spans(&params.lesson_b_text);
        Self {
            lesson_a_id: params.lesson_a_id,
            lesson_a_text: params.lesson_a_text,
            lesson_a_created_at: params.lesson_a_created_at,
            lesson_a_spans,
            lesson_b_id: params.lesson_b_id,
            lesson_b_text: params.lesson_b_text,
            lesson_b_created_at: params.lesson_b_created_at,
            lesson_b_spans,
            cosine_distance: params.cosine_distance,
        }
    }

    pub(crate) fn quote_is_verifiable(&self, quote: &str) -> bool {
        self.lesson_a_text.contains(quote) || self.lesson_b_text.contains(quote)
    }

    pub(crate) fn to_prompt(&self) -> String {
        let spans_block = |label: &str, spans: &[QuotedSpan]| -> String {
            if spans.is_empty() {
                format!("{label} polarity markers: none found\n")
            } else {
                let mut out = format!("{label} polarity markers:\n");
                for s in spans {
                    out.push_str(&format!(
                        "  - marker={:?} chars[{}..{}], in context chars[{}..{}]: {:?}\n",
                        s.marker, s.start, s.end, s.context_start, s.context_end, s.context
                    ));
                }
                out
            }
        };

        format!(
            "LESSON A (id={a_id}, created_at={a_created}):\n{a_text}\n\n{a_spans}\n\
             LESSON B (id={b_id}, created_at={b_created}):\n{b_text}\n\n{b_spans}\n\
             Cosine distance between A and B: {distance:.4}\n",
            a_id = self.lesson_a_id,
            a_created = self.lesson_a_created_at.to_rfc3339(),
            a_text = self.lesson_a_text,
            a_spans = spans_block("A", &self.lesson_a_spans),
            b_id = self.lesson_b_id,
            b_created = self.lesson_b_created_at.to_rfc3339(),
            b_text = self.lesson_b_text,
            b_spans = spans_block("B", &self.lesson_b_spans),
            distance = self.cosine_distance,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_polarity_spans_offsets_index_into_the_stored_text() {
        let text = "Earlier attempt failed. RESOLVED (commit abc123): the retry logic is FIXED. Root cause: bad lock ordering.";
        let spans = extract_polarity_spans(text);

        assert!(
            spans.iter().any(|s| s.marker == "RESOLVED"),
            "must find the RESOLVED marker"
        );
        assert!(
            spans.iter().any(|s| s.marker == "FIXED"),
            "must find the FIXED marker"
        );
        assert!(
            spans.iter().any(|s| s.marker == "root cause"),
            "must find the root cause marker case-insensitively"
        );

        for span in &spans {
            let reextracted: String = text
                .chars()
                .skip(span.start)
                .take(span.end - span.start)
                .collect();
            assert_eq!(
                reextracted, span.quote,
                "span offsets must re-extract the exact quote stored alongside them"
            );
            assert_eq!(
                reextracted.to_ascii_lowercase(),
                span.marker.to_ascii_lowercase(),
                "quote must be the marker's own extent, never padded"
            );

            let recontext: String = text
                .chars()
                .skip(span.context_start)
                .take(span.context_end - span.context_start)
                .collect();
            assert_eq!(
                recontext, span.context,
                "context offsets must re-extract the exact context stored alongside them"
            );
            assert!(
                span.context_start <= span.start && span.context_end >= span.end,
                "context must enclose the marker it surrounds"
            );
        }
    }

    #[test]
    fn extract_polarity_spans_matches_measured_lesson_29_offsets() {
        let text = "RESOLVED (commit d723bc97 on feature/mailbox-delivery-gaps, 2026-07-03): \
                    cross-session mailbox delivery landing on SUBAGENT requests (the request-88420 \
                    failure) is FIXED. Root cause: extract_turn_origin (crates/claude-pr";
        let spans = extract_polarity_spans(text);

        let Some(resolved) = spans
            .iter()
            .find(|s| s.marker.eq_ignore_ascii_case("resolved"))
        else {
            panic!("must find the RESOLVED marker");
        };
        assert_eq!((resolved.start, resolved.end), (0, 8));
        assert_eq!(resolved.quote, "RESOLVED");
        assert_eq!(&text[resolved.start..resolved.end], "RESOLVED");

        let Some(fixed) = spans
            .iter()
            .find(|s| s.marker.eq_ignore_ascii_case("fixed"))
        else {
            panic!("must find the FIXED marker");
        };
        assert_eq!((fixed.start, fixed.end), (164, 169));
        assert_eq!(fixed.quote, "FIXED");

        let Some(root_cause) = spans
            .iter()
            .find(|s| s.marker.eq_ignore_ascii_case("root cause"))
        else {
            panic!("must find the Root cause marker");
        };
        assert_eq!((root_cause.start, root_cause.end), (171, 181));
        assert_eq!(root_cause.quote, "Root cause");

        let lesson_20_text = "Cross-session mailbox delivery can land in SUBAGENT requests, not \
                               the session's main loop. Subagent/worker requests carry the same \
                               session id, and the mailbox reminder path picks the session's next \
                               eligible request without filtering by";
        assert!(
            extract_polarity_spans(lesson_20_text).is_empty(),
            "lesson 20's text must carry zero polarity markers"
        );
    }

    #[test]
    fn extract_polarity_spans_finds_no_markers_in_plain_text() {
        let spans = extract_polarity_spans("Prefer borrowing over cloning in hot paths.");
        assert!(spans.is_empty());
    }

    #[test]
    fn quote_is_verifiable_rejects_a_fabricated_quote() {
        let bundle = EvidenceBundle::build(EvidenceBundleParams {
            lesson_a_id: 1,
            lesson_a_text: "lesson A text".to_string(),
            lesson_a_created_at: Utc::now(),
            lesson_b_id: 2,
            lesson_b_text: "lesson B text".to_string(),
            lesson_b_created_at: Utc::now(),
            cosine_distance: 0.1,
        });
        assert!(bundle.quote_is_verifiable("lesson A text"));
        assert!(bundle.quote_is_verifiable("lesson B text"));
        assert!(!bundle.quote_is_verifiable("this string appears in neither lesson"));
    }
}
