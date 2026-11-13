use crate::store::persistence::lesson_relations::CandidateLessonPair;

use super::evidence;

pub fn find_artifact_reference(
    citing_text: &str,
    cited_lesson_id: i64,
) -> Option<(String, usize, usize)> {
    let pattern = format!(r"(?i)\blesson\s*#?\s*{id}\b|#{id}\b", id = cited_lesson_id);
    let re = regex::Regex::new(&pattern).ok()?;
    let m = re.find(citing_text)?;
    let start = citing_text[..m.start()].chars().count();
    let end = citing_text[..m.end()].chars().count();
    Some((m.as_str().to_string(), start, end))
}

pub fn shared_subject_tokens(text_a: &str, text_b: &str) -> Vec<String> {
    let tokens = |t: &str| -> std::collections::BTreeSet<String> {
        t.split(|c: char| !c.is_ascii_alphabetic())
            .filter(|w| w.len() >= 4)
            .map(str::to_ascii_lowercase)
            .collect()
    };
    tokens(text_a)
        .intersection(&tokens(text_b))
        .cloned()
        .collect()
}

const MIN_SHARED_SUBJECT_TOKENS: usize = 8;

pub fn find_resolution_supersession(pair: &CandidateLessonPair) -> Option<(i64, i64)> {
    let a_spans = evidence::extract_polarity_spans(&pair.lesson_a_text);
    let b_spans = evidence::extract_polarity_spans(&pair.lesson_b_text);

    let (marked_id, marked_created_at, unmarked_id, unmarked_created_at) =
        match (a_spans.is_empty(), b_spans.is_empty()) {
            (false, true) => (
                pair.lesson_a_id,
                pair.lesson_a_created_at,
                pair.lesson_b_id,
                pair.lesson_b_created_at,
            ),
            (true, false) => (
                pair.lesson_b_id,
                pair.lesson_b_created_at,
                pair.lesson_a_id,
                pair.lesson_a_created_at,
            ),
            _ => return None,
        };

    if marked_created_at <= unmarked_created_at {
        return None;
    }
    if shared_subject_tokens(&pair.lesson_a_text, &pair.lesson_b_text).len()
        < MIN_SHARED_SUBJECT_TOKENS
    {
        return None;
    }

    Some((marked_id, unmarked_id))
}
